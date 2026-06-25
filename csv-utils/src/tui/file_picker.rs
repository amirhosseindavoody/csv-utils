use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::tui::command_line::{CommandKeyAction, CommandLineState, PICKER_COMMANDS};

pub fn resolve_path(path_str: &str, base: Option<&std::path::Path>) -> io::Result<PathBuf> {
    let path = PathBuf::from(path_str);
    let abs = if path.is_absolute() {
        path
    } else if let Some(base) = base.filter(|p| !p.as_os_str().is_empty()) {
        base.join(path)
    } else {
        std::env::current_dir()?.join(path)
    };
    normalize_dir(abs)
}

fn normalize_dir(path: PathBuf) -> io::Result<PathBuf> {
    let abs = if path.is_absolute() {
        path
    } else if path.as_os_str().is_empty() {
        std::env::current_dir()?
    } else {
        std::env::current_dir()?.join(path)
    };
    match abs.canonicalize() {
        Ok(canonical) => Ok(canonical),
        Err(_) => Ok(abs),
    }
}

#[derive(Debug, Clone)]
struct Entry {
    name: String,
    is_dir: bool,
}

pub struct FilePicker {
    current_dir: PathBuf,
    entries: Vec<Entry>,
    selected: usize,
    list_offset: usize,
    error: Option<String>,
    file_extensions: Vec<String>,
    show_all: bool,
    command_line: Option<CommandLineState>,
    command_error: Option<String>,
}

impl FilePicker {
    pub fn new(file_extensions: Vec<String>) -> io::Result<Self> {
        Self::in_dir(std::env::current_dir()?, file_extensions, None)
    }

    pub fn in_dir(
        dir: PathBuf,
        file_extensions: Vec<String>,
        highlight_file: Option<PathBuf>,
    ) -> io::Result<Self> {
        let current_dir = if dir.is_dir() {
            normalize_dir(dir)?
        } else if let Some(parent) = dir.parent().filter(|p| !p.as_os_str().is_empty()) {
            normalize_dir(parent.to_path_buf())?
        } else {
            std::env::current_dir()?
        };
        let mut picker = Self {
            current_dir,
            entries: Vec::new(),
            selected: 0,
            list_offset: 0,
            error: None,
            file_extensions,
            show_all: false,
            command_line: None,
            command_error: None,
        };
        picker.refresh()?;
        if let Some(path) = highlight_file {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if let Some(idx) = picker
                    .entries
                    .iter()
                    .position(|e| !e.is_dir && e.name == name)
                {
                    picker.selected = idx;
                    picker.clamp_list_offset(1);
                }
            }
        }
        Ok(picker)
    }

    pub fn needs_picker(file_path: &Option<PathBuf>) -> bool {
        file_path.is_none()
    }

    fn file_matches_filter(&self, name: &str) -> bool {
        if self.show_all {
            return true;
        }
        let lower = name.to_lowercase();
        self.file_extensions.iter().any(|ext| lower.ends_with(&format!(".{ext}")))
    }

    pub fn refresh(&mut self) -> io::Result<()> {
        self.error = None;
        self.entries.clear();
        self.current_dir = normalize_dir(self.current_dir.clone())?;

        let read_dir = match fs::read_dir(&self.current_dir) {
            Ok(rd) => rd,
            Err(err) => {
                self.error = Some(format!("Cannot read directory: {err}"));
                self.selected = 0;
                self.list_offset = 0;
                return Ok(());
            }
        };

        let mut dirs = Vec::new();
        let mut files = Vec::new();
        for entry in read_dir.flatten() {
            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if file_type.is_dir() {
                dirs.push(Entry { name, is_dir: true });
            } else if file_type.is_file() && self.file_matches_filter(&name) {
                files.push(Entry { name, is_dir: false });
            }
        }

        dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        self.entries.extend(dirs);
        self.entries.extend(files);

        if self.entries.is_empty() {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(self.entries.len() - 1);
        }
        self.clamp_list_offset(1);
        Ok(())
    }

    fn clamp_list_offset(&mut self, visible_height: usize) {
        let max_offset = self.entries.len().saturating_sub(visible_height.max(1));
        self.list_offset = self.list_offset.min(max_offset);
        if self.selected < self.list_offset {
            self.list_offset = self.selected;
        } else if visible_height > 0 && self.selected >= self.list_offset + visible_height {
            self.list_offset = self.selected - visible_height + 1;
        }
    }

    fn go_parent(&mut self) {
        let Ok(abs) = normalize_dir(self.current_dir.clone()) else {
            return;
        };
        let Some(parent) = abs.parent() else {
            return;
        };
        if parent.as_os_str().is_empty() {
            return;
        }
        self.current_dir = parent.to_path_buf();
        self.selected = 0;
        self.list_offset = 0;
        let _ = self.refresh();
    }

    fn enter_selected_dir(&mut self) -> bool {
        let Some(entry) = self.entries.get(self.selected) else {
            return false;
        };
        if !entry.is_dir {
            return false;
        }
        self.current_dir = self.current_dir.join(&entry.name);
        self.selected = 0;
        self.list_offset = 0;
        let _ = self.refresh();
        true
    }

    fn resolve_user_path(&self, path_str: &str) -> PathBuf {
        resolve_path(path_str, Some(&self.current_dir)).unwrap_or_else(|_| self.current_dir.clone())
    }

    fn handle_command_submit(&mut self, cmd: &str) -> FilePickerAction {
        if let Some(path_str) = cmd.strip_prefix(":open ") {
            let path_str = path_str.trim();
            if path_str.is_empty() {
                self.command_error = Some(":open requires a file path".to_string());
                return FilePickerAction::Continue;
            }
            let resolved = self.resolve_user_path(path_str);
            if resolved.is_file() {
                self.command_line = None;
                return FilePickerAction::Open(resolved);
            }
            self.command_error = Some(format!("Not a file: {}", resolved.display()));
            return FilePickerAction::Continue;
        }

        match cmd {
            ":all" => {
                self.show_all = true;
                self.command_line = None;
                let _ = self.refresh();
            }
            ":filter" => {
                self.show_all = false;
                self.command_line = None;
                let _ = self.refresh();
            }
            _ => {
                self.command_error = Some(format!("Unknown command: {cmd}"));
            }
        }
        FilePickerAction::Continue
    }

    pub fn handle_key(&mut self, key: KeyEvent, visible_height: usize) -> FilePickerAction {
        if let Some(command) = self.command_line.as_mut() {
            match command.handle_key(key, PICKER_COMMANDS) {
                CommandKeyAction::Continue => {
                    self.command_error = None;
                }
                CommandKeyAction::Cancel => {
                    self.command_line = None;
                    self.command_error = None;
                }
                CommandKeyAction::Rejected => {
                    self.command_error = Some(
                        command.rejection_message(PICKER_COMMANDS).to_string(),
                    );
                }
                CommandKeyAction::Submit(cmd) => {
                    self.command_error = None;
                    return self.handle_command_submit(&cmd);
                }
            }
            return FilePickerAction::Continue;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => FilePickerAction::Quit,
            KeyCode::Char(':') => {
                self.command_line = Some(CommandLineState::start());
                self.command_error = None;
                self.error = None;
                FilePickerAction::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                self.clamp_list_offset(visible_height);
                FilePickerAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.entries.is_empty() {
                    self.selected = (self.selected + 1).min(self.entries.len() - 1);
                }
                self.clamp_list_offset(visible_height);
                FilePickerAction::Continue
            }
            KeyCode::PageUp => {
                self.selected = self.selected.saturating_sub(visible_height.max(1));
                self.clamp_list_offset(visible_height);
                FilePickerAction::Continue
            }
            KeyCode::PageDown => {
                if !self.entries.is_empty() {
                    self.selected = (self.selected + visible_height.max(1)).min(self.entries.len() - 1);
                }
                self.clamp_list_offset(visible_height);
                FilePickerAction::Continue
            }
            KeyCode::Left => {
                self.go_parent();
                FilePickerAction::Continue
            }
            KeyCode::Right => self.activate(),
            KeyCode::Enter => self.activate(),
            _ => FilePickerAction::Continue,
        }
    }

    fn activate(&mut self) -> FilePickerAction {
        let Some(entry) = self.entries.get(self.selected) else {
            return FilePickerAction::Continue;
        };

        if entry.is_dir {
            self.enter_selected_dir();
            FilePickerAction::Continue
        } else {
            FilePickerAction::Open(self.current_dir.join(&entry.name))
        }
    }

    pub fn handle_click(&mut self, row: u16, list_area: Rect) -> FilePickerAction {
        let inner = Block::default().borders(Borders::ALL).inner(list_area);
        if row < inner.y || row >= inner.y + inner.height {
            return FilePickerAction::Continue;
        }
        let rel = (row - inner.y) as usize;
        let idx = self.list_offset + rel;
        if idx >= self.entries.len() {
            return FilePickerAction::Continue;
        }
        self.selected = idx;
        self.activate()
    }

    fn filter_label(&self) -> String {
        if self.show_all {
            "all files".to_string()
        } else if self.file_extensions.is_empty() {
            "no extensions configured".to_string()
        } else {
            let exts = self
                .file_extensions
                .iter()
                .map(|ext| format!(".{ext}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("filtered ({exts})")
        }
    }

    pub fn draw(&self, frame: &mut ratatui::Frame, area: Rect, visible_height: usize) {
        let bottom_height = if self.command_line.is_some() {
            self.command_line
                .as_ref()
                .map(|c| c.panel_height(PICKER_COMMANDS))
                .unwrap_or(3)
        } else {
            1
        };
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(bottom_height),
            ])
            .split(area);

        let dir_display = self.current_dir.display().to_string();
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(dir_display),
                Line::from(Span::styled(
                    self.filter_label(),
                    Style::default().fg(Color::DarkGray),
                )),
            ])
            .block(
                Block::default()
                    .title(" Open file ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .wrap(Wrap { trim: true }),
            layout[0],
        );

        let mut lines = Vec::new();
        if let Some(err) = &self.error {
            lines.push(Line::from(Span::styled(
                err.clone(),
                Style::default().fg(Color::Red),
            )));
        } else if self.entries.is_empty() {
            if self.show_all {
                lines.push(Line::from("(empty directory)"));
            } else {
                lines.push(Line::from("(no matching files — type :all to show all)"));
            }
        } else {
            for i in 0..visible_height {
                let idx = self.list_offset + i;
                let Some(entry) = self.entries.get(idx) else {
                    lines.push(Line::from(" "));
                    continue;
                };
                let marker = if idx == self.selected { "▸ " } else { "  " };
                let suffix = if entry.is_dir { "/" } else { "" };
                let style = if idx == self.selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else if entry.is_dir {
                    Style::default().fg(Color::Blue)
                } else {
                    Style::default()
                };
                lines.push(Line::from(Span::styled(
                    format!("{marker}{}{suffix}", entry.name),
                    style,
                )));
            }
        }

        frame.render_widget(
            Paragraph::new(lines).block(
                Block::default()
                    .title(" Files ")
                    .borders(Borders::ALL),
            ),
            layout[1],
        );

        if let Some(command) = &self.command_line {
            command.draw(frame, layout[2], PICKER_COMMANDS, self.command_error.as_deref());
        } else {
            let hints = " ↑↓ navigate  → open  ← parent  Enter open  : command  q quit ";
            frame.render_widget(
                Paragraph::new(hints).style(Style::default().fg(Color::DarkGray)),
                layout[2],
            );
        }
    }
}

pub enum FilePickerAction {
    Continue,
    Open(PathBuf),
    Quit,
}
