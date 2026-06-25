use crossterm::event::{KeyCode, KeyEvent};
use csv_utils_core::model::AppModel;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

#[derive(Debug, Clone)]
pub struct ColumnFinderState {
    pub buf: String,
    suggestion_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnFinderAction {
    Continue,
    Cancel,
    Select(usize),
}

impl ColumnFinderState {
    pub fn start() -> Self {
        Self {
            buf: "/".to_string(),
            suggestion_index: 0,
        }
    }

    pub fn query(&self) -> &str {
        self.buf.strip_prefix('/').unwrap_or("").trim()
    }

    pub fn sync_filter(&self, model: &mut AppModel) {
        model.set_column_name_filter(self.query().to_string());
    }

    pub fn panel_height(&self, model: &AppModel) -> u16 {
        let matches = model.sidebar_columns_for_filter(self.query());
        let lines = 1 + matches.len().clamp(1, 5);
        (lines + 2) as u16
    }

    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        model: &mut AppModel,
        column_list_height: usize,
    ) -> ColumnFinderAction {
        self.sync_filter(model);
        let matches = model.filtered_sidebar_columns();
        match key.code {
            KeyCode::Esc => ColumnFinderAction::Cancel,
            KeyCode::Enter => {
                if matches.is_empty() {
                    return ColumnFinderAction::Continue;
                }
                let col = matches[self.clamped_suggestion_index(matches.len())];
                model.select_sidebar_column(col, column_list_height);
                ColumnFinderAction::Select(col)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if !matches.is_empty() {
                    self.suggestion_index = self
                        .suggestion_index
                        .checked_sub(1)
                        .unwrap_or(matches.len() - 1);
                }
                ColumnFinderAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !matches.is_empty() {
                    self.suggestion_index = (self.suggestion_index + 1) % matches.len();
                }
                ColumnFinderAction::Continue
            }
            KeyCode::Backspace => {
                self.buf.pop();
                self.suggestion_index = 0;
                if self.buf.is_empty() {
                    return ColumnFinderAction::Cancel;
                }
                self.sync_filter(model);
                ColumnFinderAction::Continue
            }
            KeyCode::Char(c) if !c.is_ascii_control() => {
                self.buf.push(c);
                self.suggestion_index = 0;
                self.sync_filter(model);
                ColumnFinderAction::Continue
            }
            _ => ColumnFinderAction::Continue,
        }
    }

    fn clamped_suggestion_index(&self, match_count: usize) -> usize {
        if match_count == 0 {
            0
        } else {
            self.suggestion_index.min(match_count - 1)
        }
    }

    pub fn draw(&self, frame: &mut ratatui::Frame, area: Rect, model: &AppModel) {
        let headers = model.preview.headers();
        let matches = model.filtered_sidebar_columns();
        let mut lines = vec![Line::from(Span::styled(
            self.buf.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        ))];

        if matches.is_empty() {
            lines.push(Line::from(Span::styled(
                "(no matching columns)",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            for (idx, col) in matches.iter().take(5).enumerate() {
                let selected = idx == self.clamped_suggestion_index(matches.len());
                let name = headers.get(*col).map(String::as_str).unwrap_or("");
                let label = model.format_sidebar_column_label(*col, name);
                let marker = if selected { "▸ " } else { "  " };
                let name_style = if selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Magenta)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Magenta)
                };
                lines.push(Line::from(vec![
                    Span::raw(marker),
                    Span::styled(label, name_style),
                ]));
            }
        }

        frame.render_widget(
            Paragraph::new(lines).block(
                Block::default()
                    .title(" Column finder ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Magenta)),
            ),
            area,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState};
    use std::path::PathBuf;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: crossterm::event::KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        }
    }

    #[test]
    fn filters_columns_as_user_types() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        let mut finder = ColumnFinderState::start();
        finder.buf = "/city".to_string();
        finder.sync_filter(&mut model);
        let matches = model.filtered_sidebar_columns();
        assert!(!matches.is_empty());
        let headers = model.preview.headers();
        assert!(matches
            .iter()
            .all(|&col| headers[col].to_ascii_lowercase().contains("city")));
    }

    #[test]
    fn enter_selects_highlighted_column() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        let mut finder = ColumnFinderState {
            buf: "/0".to_string(),
            suggestion_index: 0,
        };
        let action = finder.handle_key(press(KeyCode::Enter), &mut model, 10);
        assert_eq!(action, ColumnFinderAction::Select(0));
        assert_eq!(model.view.selected_col, 0);
    }
}
