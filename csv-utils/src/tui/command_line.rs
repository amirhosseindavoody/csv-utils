use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

pub struct CommandSpec {
    pub primary: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
    /// When true, Enter selects the command first, then waits for a path argument.
    pub takes_args: bool,
}

impl CommandSpec {
    pub fn names(&self) -> Vec<&'static str> {
        let mut names = vec![self.primary];
        names.extend_from_slice(self.aliases);
        names
    }

    fn arg_prefix(&self) -> String {
        format!("{} ", self.primary)
    }
}

pub const PICKER_COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        primary: ":all",
        aliases: &[":a"],
        description: "Show all files",
        takes_args: false,
    },
    CommandSpec {
        primary: ":filter",
        aliases: &[":f"],
        description: "Apply extension filter from settings (when configured)",
        takes_args: false,
    },
    CommandSpec {
        primary: ":open",
        aliases: &[],
        description: "Open file by path",
        takes_args: true,
    },
];

pub const VIEW_COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        primary: ":open",
        aliases: &[],
        description: "Open file or browse directory by path",
        takes_args: true,
    },
    CommandSpec {
        primary: ":close",
        aliases: &[],
        description: "Close file and open file picker",
        takes_args: false,
    },
    CommandSpec {
        primary: ":toggle-borders",
        aliases: &[],
        description: "Show or hide table column border lines",
        takes_args: false,
    },
    CommandSpec {
        primary: ":filter",
        aliases: &[":f"],
        description: "Filter rows (selected column) or sidebar when focused",
        takes_args: true,
    },
    CommandSpec {
        primary: ":hide",
        aliases: &[":h"],
        description: "Hide selected rows (table) or columns (sidebar)",
        takes_args: false,
    },
    CommandSpec {
        primary: ":unhide",
        aliases: &[":u"],
        description: "Unhide selected or all hidden rows/columns",
        takes_args: false,
    },
    CommandSpec {
        primary: ":sort",
        aliases: &[],
        description: "Sort rows by selected column (asc → desc → clear)",
        takes_args: true,
    },
    CommandSpec {
        primary: ":web",
        aliases: &[],
        description: "Open browser UI on a free local port and exit terminal view",
        takes_args: false,
    },
];

#[derive(Debug, Clone)]
pub struct CommandLineState {
    pub buf: String,
    suggestion_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandKeyAction {
    Continue,
    Cancel,
    Submit(String),
    Rejected,
}

impl CommandLineState {
    pub fn start() -> Self {
        Self {
            buf: ":".to_string(),
            suggestion_index: 0,
        }
    }

    pub fn in_args_entry_mode(&self, commands: &[CommandSpec]) -> bool {
        commands.iter().any(|cmd| {
            if !cmd.takes_args {
                return false;
            }
            if self.buf.starts_with(&cmd.arg_prefix()) {
                return true;
            }
            cmd.aliases.iter().any(|alias| self.buf.starts_with(&format!("{alias} ")))
        })
    }

    pub fn rejection_message(&self, commands: &[CommandSpec]) -> &'static str {
        if self.in_args_entry_mode(commands) {
            "Path required"
        } else {
            "No matching command"
        }
    }

    pub fn filtered<'a>(&self, commands: &'a [CommandSpec]) -> Vec<&'a CommandSpec> {
        if self.in_args_entry_mode(commands) {
            return Vec::new();
        }
        let query = self.buf.trim().to_ascii_lowercase();
        if query == ":" {
            return commands.iter().collect();
        }
        let command_part = query.split_whitespace().next().unwrap_or(&query);
        commands
            .iter()
            .filter(|cmd| {
                cmd.names().iter().any(|name| {
                    name.to_ascii_lowercase().starts_with(command_part)
                })
            })
            .collect()
    }

    pub fn panel_height(&self, commands: &[CommandSpec]) -> u16 {
        if self.in_args_entry_mode(commands) {
            // Input line + hint/error line, plus top/bottom borders.
            return 4;
        }
        let matches = self.filtered(commands);
        let lines = 1 + matches.len().clamp(1, 5);
        (lines + 2) as u16
    }

    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        commands: &[CommandSpec],
    ) -> CommandKeyAction {
        let matches = self.filtered(commands);
        match key.code {
            KeyCode::Esc => CommandKeyAction::Cancel,
            KeyCode::Enter => {
                if self.in_args_entry_mode(commands) {
                    return if let Some(submitted) = self.resolve_args_submit(commands) {
                        CommandKeyAction::Submit(submitted)
                    } else {
                        CommandKeyAction::Rejected
                    };
                }
                if let Some(cmd) = self.resolve_select_for_enter(&matches) {
                    if cmd.takes_args {
                        self.buf = cmd.arg_prefix();
                        self.suggestion_index = 0;
                        return CommandKeyAction::Continue;
                    }
                    return CommandKeyAction::Submit(cmd.primary.to_string());
                }
                CommandKeyAction::Rejected
            }
            KeyCode::Tab => {
                if !self.in_args_entry_mode(commands) {
                    self.autocomplete(&matches);
                }
                CommandKeyAction::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if !self.in_args_entry_mode(commands) && !matches.is_empty() {
                    self.suggestion_index = self
                        .suggestion_index
                        .checked_sub(1)
                        .unwrap_or(matches.len() - 1);
                }
                CommandKeyAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.in_args_entry_mode(commands) && !matches.is_empty() {
                    self.suggestion_index = (self.suggestion_index + 1) % matches.len();
                }
                CommandKeyAction::Continue
            }
            KeyCode::Backspace => {
                self.buf.pop();
                self.suggestion_index = 0;
                if self.buf.is_empty() {
                    return CommandKeyAction::Cancel;
                }
                CommandKeyAction::Continue
            }
            KeyCode::Char(c) if !c.is_ascii_control() => {
                self.buf.push(c);
                self.suggestion_index = 0;
                CommandKeyAction::Continue
            }
            _ => CommandKeyAction::Continue,
        }
    }

    fn resolve_args_submit(&self, commands: &[CommandSpec]) -> Option<String> {
        for cmd in commands {
            if !cmd.takes_args {
                continue;
            }
            let prefix = cmd.arg_prefix();
            if self.buf.starts_with(&prefix) {
                let args = self.buf[prefix.len()..].trim();
                if !args.is_empty() {
                    return Some(self.buf.trim().to_string());
                }
                return None;
            }
        }
        None
    }

    fn clamped_suggestion_index(&self, match_count: usize) -> usize {
        if match_count == 0 {
            0
        } else {
            self.suggestion_index.min(match_count - 1)
        }
    }

    fn resolve_select_for_enter<'a>(
        &self,
        matches: &'a [&'a CommandSpec],
    ) -> Option<&'a CommandSpec> {
        let query = self.buf.trim().to_ascii_lowercase();

        for cmd in matches {
            if cmd
                .names()
                .iter()
                .any(|name| name.to_ascii_lowercase() == query)
            {
                return Some(cmd);
            }
        }

        if !matches.is_empty() {
            return Some(matches[self.clamped_suggestion_index(matches.len())]);
        }

        None
    }

    fn autocomplete(&mut self, matches: &[&CommandSpec]) {
        let Some(cmd) = matches
            .get(self.suggestion_index)
            .copied()
            .or_else(|| matches.first().copied())
        else {
            return;
        };
        if cmd.takes_args {
            if !self.buf.starts_with(&cmd.arg_prefix()) {
                self.buf = cmd.arg_prefix();
            }
        } else {
            self.buf = cmd.primary.to_string();
        }
    }

    pub fn draw(
        &self,
        frame: &mut ratatui::Frame,
        area: Rect,
        commands: &[CommandSpec],
        error: Option<&str>,
    ) {
        let matches = self.filtered(commands);
        let mut lines = vec![Line::from(Span::styled(
            self.buf.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        ))];

        if let Some(err) = error {
            lines.push(Line::from(Span::styled(
                err.to_string(),
                Style::default().fg(Color::Red),
            )));
        } else if self.in_args_entry_mode(commands) {
            lines.push(Line::from(Span::styled(
                "Type or paste path, then Enter",
                Style::default().fg(Color::DarkGray),
            )));
        } else if matches.is_empty() {
            lines.push(Line::from(Span::styled(
                "(no matching commands)",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            for (idx, cmd) in matches.iter().take(5).enumerate() {
                let selected = idx == self.clamped_suggestion_index(matches.len());
                let marker = if selected { "▸ " } else { "  " };
                let name_style = if selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Cyan)
                };
                let alias_hint = if cmd.aliases.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", cmd.aliases.join(", "))
                };
                let arg_hint = if cmd.takes_args { " …" } else { "" };
                lines.push(Line::from(vec![
                    Span::raw(marker),
                    Span::styled(
                        format!("{}{alias_hint}{arg_hint}", cmd.primary),
                        name_style,
                    ),
                    Span::styled(
                        format!("  {}", cmd.description),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }
        }

        frame.render_widget(
            Paragraph::new(lines).block(
                Block::default()
                    .title(" Command ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow)),
            ),
            area,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState};

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: crossterm::event::KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        }
    }

    #[test]
    fn filters_commands_by_prefix() {
        let state = CommandLineState {
            buf: ":a".to_string(),
            suggestion_index: 0,
        };
        let matches = state.filtered(PICKER_COMMANDS);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].primary, ":all");
    }

    #[test]
    fn shows_all_commands_for_colon_only() {
        let state = CommandLineState {
            buf: ":".to_string(),
            suggestion_index: 0,
        };
        assert_eq!(state.filtered(PICKER_COMMANDS).len(), 3);
        assert_eq!(state.filtered(VIEW_COMMANDS).len(), 8);
    }

    #[test]
    fn selects_open_then_waits_for_path() {
        let mut state = CommandLineState {
            buf: ":open".to_string(),
            suggestion_index: 0,
        };
        let action = state.handle_key(press(KeyCode::Enter), PICKER_COMMANDS);
        assert_eq!(action, CommandKeyAction::Continue);
        assert_eq!(state.buf, ":open ");
        assert!(state.in_args_entry_mode(PICKER_COMMANDS));

        state.buf.push_str("data/file.csv");
        let action = state.handle_key(press(KeyCode::Enter), PICKER_COMMANDS);
        assert_eq!(
            action,
            CommandKeyAction::Submit(":open data/file.csv".to_string())
        );
    }

    #[test]
    fn rejects_empty_path_after_selecting_open() {
        let mut state = CommandLineState {
            buf: ":open ".to_string(),
            suggestion_index: 0,
        };
        let action = state.handle_key(press(KeyCode::Enter), PICKER_COMMANDS);
        assert_eq!(action, CommandKeyAction::Rejected);
        assert_eq!(state.rejection_message(PICKER_COMMANDS), "Path required");
    }

    #[test]
    fn submits_close_without_args_step() {
        let mut state = CommandLineState {
            buf: ":close".to_string(),
            suggestion_index: 0,
        };
        let action = state.handle_key(press(KeyCode::Enter), VIEW_COMMANDS);
        assert_eq!(action, CommandKeyAction::Submit(":close".to_string()));
    }

    #[test]
    fn panel_height_fits_args_entry_without_extra_line() {
        let state = CommandLineState {
            buf: ":open sadfas".to_string(),
            suggestion_index: 0,
        };
        assert!(state.in_args_entry_mode(VIEW_COMMANDS));
        assert_eq!(state.panel_height(VIEW_COMMANDS), 4);
    }

    #[test]
    fn submits_toggle_borders_without_args_step() {
        let mut state = CommandLineState {
            buf: ":toggle-borders".to_string(),
            suggestion_index: 0,
        };
        let action = state.handle_key(press(KeyCode::Enter), VIEW_COMMANDS);
        assert_eq!(
            action,
            CommandKeyAction::Submit(":toggle-borders".to_string())
        );
    }

    #[test]
    fn submits_hide_without_args() {
        let mut state = CommandLineState {
            buf: ":hide".to_string(),
            suggestion_index: 0,
        };
        let action = state.handle_key(press(KeyCode::Enter), VIEW_COMMANDS);
        assert_eq!(action, CommandKeyAction::Submit(":hide".to_string()));
    }

    #[test]
    fn submits_hide_alias_without_args() {
        let mut state = CommandLineState {
            buf: ":h".to_string(),
            suggestion_index: 0,
        };
        let action = state.handle_key(press(KeyCode::Enter), VIEW_COMMANDS);
        assert_eq!(action, CommandKeyAction::Submit(":hide".to_string()));
    }

    #[test]
    fn submits_unhide_without_args() {
        let mut state = CommandLineState {
            buf: ":unhide".to_string(),
            suggestion_index: 0,
        };
        let action = state.handle_key(press(KeyCode::Enter), VIEW_COMMANDS);
        assert_eq!(action, CommandKeyAction::Submit(":unhide".to_string()));
    }

    #[test]
    fn selects_highlighted_command_when_only_colon_typed() {
        let mut state = CommandLineState {
            buf: ":".to_string(),
            suggestion_index: 0,
        };
        state.handle_key(press(KeyCode::Down), PICKER_COMMANDS);
        let action = state.handle_key(press(KeyCode::Enter), PICKER_COMMANDS);
        assert_eq!(action, CommandKeyAction::Submit(":filter".to_string()));
    }

    #[test]
    fn selects_highlighted_open_from_colon_via_arrows() {
        let mut state = CommandLineState {
            buf: ":".to_string(),
            suggestion_index: 0,
        };
        state.handle_key(press(KeyCode::Down), PICKER_COMMANDS);
        state.handle_key(press(KeyCode::Down), PICKER_COMMANDS);
        let action = state.handle_key(press(KeyCode::Enter), PICKER_COMMANDS);
        assert_eq!(action, CommandKeyAction::Continue);
        assert_eq!(state.buf, ":open ");
    }
}
