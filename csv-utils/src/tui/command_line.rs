use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

pub struct CommandSpec {
    pub primary: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
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
    },
    CommandSpec {
        primary: ":filter",
        aliases: &[":f"],
        description: "Show only configured extensions",
    },
];

pub const VIEW_COMMANDS: &[CommandSpec] = &[CommandSpec {
    primary: ":close",
    aliases: &[],
    description: "Close file and open file picker",
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
        commands
            .iter()
            .filter(|cmd| {
                cmd.names()
                    .iter()
                    .any(|name| name.to_ascii_lowercase().starts_with(&query))
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
                if let Some(primary) = self.resolve_submit(&matches) {
                    CommandKeyAction::Submit(primary.to_string())
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

    fn resolve_submit(&self, matches: &[&CommandSpec]) -> Option<&'static str> {
        let query = self.buf.trim().to_ascii_lowercase();
        for cmd in matches {
            if cmd
                .names()
                .iter()
                .any(|name| name.to_ascii_lowercase() == query)
            {
                return Some(cmd.primary);
            }
        }
        if matches.len() == 1 {
            return Some(matches[0].primary);
        }
        matches
            .get(self.suggestion_index)
            .copied()
            .map(|cmd| cmd.primary)
    }

    fn autocomplete(&mut self, matches: &[&CommandSpec]) {
        let Some(cmd) = matches.get(self.suggestion_index).copied().or_else(|| matches.first().copied())
        else {
            return;
        };
        self.buf = cmd.primary.to_string();
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
        assert_eq!(state.filtered(PICKER_COMMANDS).len(), 2);
        assert_eq!(state.filtered(VIEW_COMMANDS).len(), 1);
    }

    #[test]
    fn resolves_single_match_on_submit() {
        let state = CommandLineState {
            buf: ":cl".to_string(),
            suggestion_index: 0,
        };
        let matches = state.filtered(VIEW_COMMANDS);
        assert_eq!(state.resolve_submit(&matches), Some(":close"));
    }
}
