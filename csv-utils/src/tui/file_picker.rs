use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

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
}

impl FilePicker {
    pub fn new() -> io::Result<Self> {
        let current_dir = std::env::current_dir()?;
        let mut picker = Self {
            current_dir,
            entries: Vec::new(),
            selected: 0,
            list_offset: 0,
            error: None,
        };
        picker.refresh()?;
        Ok(picker)
    }

    pub fn needs_picker(file_path: &Option<PathBuf>) -> bool {
        file_path.is_none()
    }

    pub fn refresh(&mut self) -> io::Result<()> {
        self.error = None;
        self.entries.clear();

        if self.current_dir.parent().is_some() {
            self.entries.push(Entry {
                name: "..".to_string(),
                is_dir: true,
            });
        }

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
            } else if file_type.is_file() {
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

    pub fn handle_key(&mut self, key: KeyEvent, visible_height: usize) -> FilePickerAction {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => FilePickerAction::Quit,
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
            KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h') => {
                if self.current_dir.parent().is_some() {
                    if let Some(parent) = self.current_dir.parent() {
                        self.current_dir = parent.to_path_buf();
                        self.selected = 0;
                        self.list_offset = 0;
                        let _ = self.refresh();
                    }
                }
                FilePickerAction::Continue
            }
            KeyCode::Enter => self.activate(),
            _ => FilePickerAction::Continue,
        }
    }

    fn activate(&mut self) -> FilePickerAction {
        let Some(entry) = self.entries.get(self.selected) else {
            return FilePickerAction::Continue;
        };

        if entry.is_dir {
            let next = if entry.name == ".." {
                self.current_dir.parent().map(Path::to_path_buf)
            } else {
                Some(self.current_dir.join(&entry.name))
            };
            if let Some(path) = next {
                self.current_dir = path;
                self.selected = 0;
                self.list_offset = 0;
                let _ = self.refresh();
            }
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

    pub fn draw(&self, frame: &mut ratatui::Frame, area: Rect, visible_height: usize) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(1),
            ])
            .split(area);

        let dir_display = self.current_dir.display().to_string();
        frame.render_widget(
            Paragraph::new(dir_display)
                .block(
                    Block::default()
                        .title(" Open CSV file ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Cyan)),
                )
                .wrap(Wrap { trim: true }),
            layout[0],
        );

        let inner = Block::default().borders(Borders::ALL).inner(layout[1]);
        let mut lines = Vec::new();
        if let Some(err) = &self.error {
            lines.push(Line::from(Span::styled(
                err.clone(),
                Style::default().fg(Color::Red),
            )));
        } else if self.entries.is_empty() {
            lines.push(Line::from("(empty directory)"));
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

        let hints = " ↑↓ navigate  Enter open  Backspace parent  q quit ";
        frame.render_widget(
            Paragraph::new(hints).style(Style::default().fg(Color::DarkGray)),
            layout[2],
        );

        let _ = inner;
    }
}

pub enum FilePickerAction {
    Continue,
    Open(PathBuf),
    Quit,
}
