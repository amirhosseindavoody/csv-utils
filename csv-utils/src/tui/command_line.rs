use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

pub struct CommandSpec {
    pub primary: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
    /// When true, Enter submits the full buffer (e.g. `:open path/to/file`).
    pub takes_args: bool,
}

impl CommandSpec {
    pub fn names(&self) -> Vec<&'static str> {
        let mut names = vec![self.primary];
        names.extend_from_slice(self.aliases);
        names
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
        description: "Show only configured extensions",
        takes_args: false,
    },
    CommandSpec {
        primary: ":open",
        aliases: &[],
        description: "Open file by path  (:open path/to/file.csv)",
        takes_args: true,
    },
];

pub const VIEW_COMMANDS: &[CommandSpec] = &[CommandSpec {
    primary: ":close",
    aliases: &[],
    description: "Close file and open file picker",
    takes_args: false,
}];

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

    pub fn filtered<'a>(&self, commands: &'a [CommandSpec]) -> Vec<&'a CommandSpec> {
        let query = self.buf.trim().to_ascii_lowercase();
        if query == ":" {
            return commands.iter().collect();
        }
        let command_part = query.split_whitespace().next().unwrap_or(&query);
        commands
            .iter()
            .filter(|cmd| {
                cmd.names().iter().any(|name| {
                    name.to_ascii_lowercase()
                        .starts_with(command_part)
                })
            })
            .collect()
    }

    pub fn panel_height(&self, commands: &[CommandSpec]) -> u16 {
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
                if let Some(submitted) = self.resolve_submit(&matches) {
                    CommandKeyAction::Submit(submitted)
                } else {
                    CommandKeyAction::Rejected
                }
            }
            KeyCode::Tab => {
                self.autocomplete(&matches);
                CommandKeyAction::Continue
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if !matches.is_empty() {
                    self.suggestion_index = self
                        .suggestion_index
                        .checked_sub(1)
                        .unwrap_or(matches.len() - 1);
                }
                CommandKeyAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !matches.is_empty() {
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

    fn resolve_submit(&self, matches: &[&CommandSpec]) -> Option<String> {
        let raw = self.buf.trim();
        let query = raw.to_ascii_lowercase();

        for cmd in matches {
            if !cmd.takes_args {
                continue;
            }
            let prefix = format!("{} ", cmd.primary);
            if query.starts_with(&prefix.to_ascii_lowercase()) {
                let args = raw[prefix.len()..].trim();
                if !args.is_empty() {
                    return Some(raw.to_string());
                }
                return None;
            }
            if query == cmd.primary.to_ascii_lowercase() {
                return None;
            }
        }

        for cmd in matches {
            if cmd
                .names()
                .iter()
                .any(|name| name.to_ascii_lowercase() == query)
            {
                return Some(cmd.primary.to_string());
            }
        }
        if matches.len() == 1 && !matches[0].takes_args {
            return Some(matches[0].primary.to_string());
        }
        matches
            .get(self.suggestion_index)
            .filter(|cmd| !cmd.takes_args)
            .map(|cmd| cmd.primary.to_string())
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
            let prefix = format!("{} ", cmd.primary);
            if !self.buf.to_ascii_lowercase().starts_with(&prefix.to_ascii_lowercase()) {
                self.buf = prefix;
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
        } else if matches.is_empty() {
            lines.push(Line::from(Span::styled(
                "(no matching commands)",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            for (idx, cmd) in matches.iter().take(5).enumerate() {
                let selected = idx == self.suggestion_index;
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
                lines.push(Line::from(vec![
                    Span::raw(marker),
                    Span::styled(format!("{}{alias_hint}", cmd.primary), name_style),
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
        assert_eq!(state.filtered(VIEW_COMMANDS).len(), 1);
    }

    #[test]
    fn resolves_single_match_on_submit() {
        let state = CommandLineState {
            buf: ":cl".to_string(),
            suggestion_index: 0,
        };
        let matches = state.filtered(VIEW_COMMANDS);
        assert_eq!(state.resolve_submit(&matches), Some(":close".to_string()));
    }

    #[test]
    fn resolves_open_with_path() {
        let state = CommandLineState {
            buf: ":open data/file.csv".to_string(),
            suggestion_index: 0,
        };
        let matches = state.filtered(PICKER_COMMANDS);
        assert_eq!(
            state.resolve_submit(&matches),
            Some(":open data/file.csv".to_string())
        );
    }

    #[test]
    fn rejects_open_without_path() {
        let state = CommandLineState {
            buf: ":open".to_string(),
            suggestion_index: 0,
        };
        let matches = state.filtered(PICKER_COMMANDS);
        assert_eq!(state.resolve_submit(&matches), None);
    }
}
