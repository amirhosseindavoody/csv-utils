use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, MouseEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use csv_utils_core::column::{infer_column_kind, is_right_aligned};
use csv_utils_core::model::{format_cell, AppModel, CELL_DISPLAY_WIDTH};
use csv_utils_core::schema;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, Paragraph, Row, Table, Wrap,
};
use ratatui::Terminal;
use std::io::{self, stdout};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const HELP_TEXT: &str = "\
csv-utils — keyboard shortcuts

  q          quit
  ↑/↓        previous / next row
  ←/→        previous / next column
  PgUp/PgDn  scroll 10 rows
  t          toggle column type labels
  ?          this help

Mouse: click a cell to select row/column; wheel on table scrolls rows; wheel on column list scrolls columns.

Press Esc or ? to close.";

struct LayoutAreas {
    table: Rect,
    columns: Rect,
}

pub fn run(file: Option<&str>) -> Result<()> {
    let file_path = file.map(PathBuf::from);
    let mut model = AppModel::open(file_path)?;

    enable_raw_mode()?;
    let mut stdout = stdout();
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(crossterm::event::EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut running = true;
    let mut areas = LayoutAreas {
        table: Rect::default(),
        columns: Rect::default(),
    };
    let mut last_redraw = Instant::now();

    while running {
        terminal.draw(|frame| {
            areas = draw(frame, &model);
        })?;

        let size = terminal.size()?;
        let table_width = areas.table.width;
        let viewport_rows = areas.table.height.saturating_sub(3) as usize;
        let column_list_height = column_list_visible_height(areas.columns);
        model.clamp_selection(viewport_rows.max(1), table_width);
        model.clamp_column_list_offset(column_list_height);

        let timeout = Duration::from_millis(50);
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    running = handle_key(key, &mut model, column_list_height);
                }
                Event::Mouse(mouse) => handle_mouse(mouse, &mut model, &areas, column_list_height),
                Event::Resize(_, _) => {}
                _ => {}
            }
        } else if !model.preview.scan_done() && last_redraw.elapsed() >= Duration::from_millis(100)
        {
            last_redraw = Instant::now();
        }
        let _ = size;
    }

    disable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(crossterm::event::DisableMouseCapture)?;
    stdout.execute(LeaveAlternateScreen)?;

    if let Some(handle) = model.scan_thread.take() {
        let _ = handle.join();
    }

    Ok(())
}

fn column_list_visible_height(columns_area: Rect) -> usize {
    Block::default()
        .borders(Borders::ALL)
        .inner(columns_area)
        .height
        .max(1) as usize
}

fn handle_key(key: KeyEvent, model: &mut AppModel, column_list_height: usize) -> bool {
    if model.view.show_help {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')) {
            model.view.show_help = false;
        }
        return true;
    }

    match key.code {
        KeyCode::Char('q') => return false,
        KeyCode::Char('?') => model.view.show_help = true,
        KeyCode::Char('t') => model.view.show_column_types = !model.view.show_column_types,
        KeyCode::Up | KeyCode::Char('k') => {
            model.view.selected_row = model.view.selected_row.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            model.view.selected_row = model.view.selected_row.saturating_add(1);
        }
        KeyCode::Left | KeyCode::Char('h') => {
            model.view.selected_col = model.view.selected_col.saturating_sub(1);
            model.ensure_column_list_shows_selection(column_list_height);
        }
        KeyCode::Right | KeyCode::Char('l') => {
            model.view.selected_col = model.view.selected_col.saturating_add(1);
            model.ensure_column_list_shows_selection(column_list_height);
        }
        KeyCode::PageUp => {
            model.view.selected_row = model.view.selected_row.saturating_sub(10);
        }
        KeyCode::PageDown => {
            model.view.selected_row = model.view.selected_row.saturating_add(10);
        }
        KeyCode::Home => model.view.selected_row = 0,
        KeyCode::End => {
            model.view.selected_row = model.preview.row_count().saturating_sub(1);
        }
        _ => {}
    }
    true
}

fn handle_mouse(
    mouse: crossterm::event::MouseEvent,
    model: &mut AppModel,
    areas: &LayoutAreas,
    column_list_height: usize,
) {
    if model.view.show_help {
        return;
    }

    let row = mouse.row;
    let col = mouse.column;
    let pos = Position { x: col, y: row };

    match mouse.kind {
        MouseEventKind::ScrollUp if areas.columns.contains(pos) => {
            model.view.column_list_offset = model
                .view
                .column_list_offset
                .saturating_sub(3);
            model.clamp_column_list_offset(column_list_height);
        }
        MouseEventKind::ScrollDown if areas.columns.contains(pos) => {
            model.view.column_list_offset += 3;
            model.clamp_column_list_offset(column_list_height);
        }
        MouseEventKind::ScrollUp if areas.table.contains(pos) => {
            model.view.selected_row = model.view.selected_row.saturating_sub(3);
        }
        MouseEventKind::ScrollDown if areas.table.contains(pos) => {
            model.view.selected_row = model.view.selected_row.saturating_add(3);
        }
        MouseEventKind::Down(crossterm::event::MouseButton::Left)
            if areas.table.contains(pos) =>
        {
            if let Some(hit) = hit_test_table(col, row, areas.table, model) {
                model.view.selected_col = hit.col;
                if let Some(row_idx) = hit.row {
                    model.view.selected_row = row_idx;
                }
                model.ensure_column_list_shows_selection(column_list_height);
            }
        }
        MouseEventKind::Down(crossterm::event::MouseButton::Left)
            if areas.columns.contains(pos) =>
        {
            let inner = Block::default().borders(Borders::ALL).inner(areas.columns);
            if !inner.contains(pos) {
                return;
            }
            let rel = row.saturating_sub(inner.y) as usize;
            let idx = model.view.column_list_offset + rel;
            if idx < model.preview.headers().len() {
                model.view.selected_col = idx;
                model.ensure_column_list_shows_selection(column_list_height);
            }
        }
        _ => {}
    }
}

struct TableHit {
    row: Option<usize>,
    col: usize,
}

/// Map a screen coordinate to a table row/column (must match `draw_table` layout).
fn hit_test_table(mouse_x: u16, mouse_y: u16, table_area: Rect, model: &AppModel) -> Option<TableHit> {
    let inner = Block::default().borders(Borders::ALL).inner(table_area);
    if !inner.contains(Position {
        x: mouse_x,
        y: mouse_y,
    }) {
        return None;
    }

    let col_range = model.visible_column_range(table_area.width);
    let visible_cols = col_range.len();
    if visible_cols == 0 {
        return None;
    }

    let col_slot = CELL_DISPLAY_WIDTH as u16 + 1;
    let rel_x = mouse_x.saturating_sub(inner.x);
    let vis_col = (rel_x / col_slot) as usize;
    if vis_col >= visible_cols {
        return None;
    }
    let col_idx = col_range.start + vis_col;

    let rel_y = mouse_y.saturating_sub(inner.y);
    if rel_y == 0 {
        return Some(TableHit {
            row: None,
            col: col_idx,
        });
    }
    if rel_y == 1 {
        return None;
    }

    let data_row = (rel_y - 2) as usize;
    let row_idx = model.view.row_offset + data_row;
    if row_idx >= model.preview.row_count() {
        return None;
    }

    Some(TableHit {
        row: Some(row_idx),
        col: col_idx,
    })
}

fn draw(frame: &mut ratatui::Frame, model: &AppModel) -> LayoutAreas {
    let area = frame.area();

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(area);

    let title = format!(
        " csv-utils  │  {}  │  {} rows",
        model.file_label(),
        model.preview.row_count()
    );
    let scan_note = if model.preview.scan_error() {
        Span::styled("  ERROR loading file", Style::default().fg(Color::Red))
    } else if !model.preview.scan_done() {
        Span::styled("  loading…", Style::default().fg(Color::Yellow))
    } else {
        Span::raw("")
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(title, Style::default().add_modifier(Modifier::BOLD)),
            scan_note,
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .wrap(Wrap { trim: true }),
        outer[0],
    );

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(20), Constraint::Length(32)])
        .split(outer[1]);

    let table_area = main[0];
    let columns_area = main[1];

    draw_table(frame, table_area, model);
    draw_column_list(frame, columns_area, model);

    let hints = " q quit  ↑↓ rows  ←→ cols  t types  ? help ";
    frame.render_widget(
        Paragraph::new(hints).style(Style::default().fg(Color::DarkGray)),
        outer[2],
    );

    if model.view.show_help {
        draw_help(frame, area);
    }

    LayoutAreas {
        table: table_area,
        columns: columns_area,
    }
}

fn draw_table(frame: &mut ratatui::Frame, area: Rect, model: &AppModel) {
    let headers = model.preview.headers();
    if headers.is_empty() {
        frame.render_widget(
            Paragraph::new("Open a CSV file:\n  csv-utils tui path/to/file.csv")
                .block(Block::default().title(" Data ").borders(Borders::ALL))
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    let col_range = model.visible_column_range(area.width);
    let visible_headers: Vec<_> = col_range
        .clone()
        .map(|i| headers[i].as_str())
        .collect();

    let header = Row::new(
        visible_headers
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let col_idx = col_range.start + i;
                let style = if col_idx == model.view.selected_col {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().add_modifier(Modifier::BOLD)
                };
                Cell::from(*name).style(style)
            })
            .collect::<Vec<_>>(),
    )
    .height(1)
    .bottom_margin(1);

    let body_height = area.height.saturating_sub(3) as usize;
    let mut rows = Vec::new();
    for r in 0..body_height {
        let row_idx = model.view.row_offset + r;
        let Some(line) = model.preview.row_line(row_idx) else {
            break;
        };
        let fields = schema::split_row(&line);
        let row_selected = row_idx == model.view.selected_row;
        let cells: Vec<Cell> = col_range
            .clone()
            .enumerate()
            .map(|(_i, col_idx)| {
                let text = fields.get(col_idx).map(String::as_str).unwrap_or("");
                let kind = infer_column_kind(&headers[col_idx]);
                let display = format_cell(text, CELL_DISPLAY_WIDTH, is_right_aligned(kind));
                let mut style = Style::default();
                if row_selected {
                    style = style.bg(Color::DarkGray);
                }
                if row_selected && col_idx == model.view.selected_col {
                    style = Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD);
                }
                Cell::from(display).style(style)
            })
            .collect();
        rows.push(Row::new(cells).height(1));
    }

    let widths: Vec<Constraint> = (0..visible_headers.len())
        .map(|_| Constraint::Length(CELL_DISPLAY_WIDTH as u16))
        .collect();

    let title = format!(
        " Data  (rows {}–{}) ",
        model.view.row_offset + 1,
        (model.view.row_offset + rows.len()).min(model.preview.row_count())
    );

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().title(title).borders(Borders::ALL))
        .column_spacing(1)
        .row_highlight_style(Style::default().bg(Color::DarkGray));

    frame.render_widget(table, area);
}

fn draw_column_list(frame: &mut ratatui::Frame, area: Rect, model: &AppModel) {
    let headers = model.preview.headers();
    let inner = Block::default().borders(Borders::ALL).inner(area);
    let visible_height = inner.height.max(1) as usize;
    let mut lines = Vec::new();

    for i in 0..visible_height {
        let col_idx = model.view.column_list_offset + i;
        if let Some(name) = headers.get(col_idx) {
            let kind = infer_column_kind(name);
            let text = if model.view.show_column_types {
                format!("{col_idx}: {name} [{}]", kind.label())
            } else {
                format!("{col_idx}: {name}")
            };
            let display = format_cell(&text, inner.width as usize, false);
            let style = if col_idx == model.view.selected_col {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Magenta)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let prefix = if col_idx == model.view.selected_col {
                "▸ "
            } else {
                "  "
            };
            lines.push(Line::from(Span::styled(format!("{prefix}{display}"), style)));
        } else {
            lines.push(Line::from(" "));
        }
    }

    let end = (model.view.column_list_offset + visible_height).min(headers.len());

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(format!(
                    " Columns ({}–{}/{}) ",
                    if headers.is_empty() {
                        0
                    } else {
                        model.view.column_list_offset + 1
                    },
                    end,
                    headers.len()
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta)),
        ),
        area,
    );
}

fn draw_help(frame: &mut ratatui::Frame, area: Rect) {
    let popup_area = centered_rect(60, 70, area);
    frame.render_widget(Clear, popup_area);
    frame.render_widget(
        Paragraph::new(HELP_TEXT)
            .block(
                Block::default()
                    .title(" Help ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow)),
            )
            .wrap(Wrap { trim: true }),
        popup_area,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
