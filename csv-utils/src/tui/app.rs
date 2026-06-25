use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, MouseEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use csv_utils_core::column::{ColumnKind, NumericRepr};
use csv_utils_core::display::truncate_middle;
use csv_utils_core::model::{AppModel, MAX_COLUMN_WIDTH, MIN_COLUMN_WIDTH};
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

use crate::tui::file_picker::{FilePicker, FilePickerAction};

const HELP_TEXT: &str = "\
csv — keyboard shortcuts

  q          quit (closes a panel when one is open)
  ↑/↓        previous / next row
  ←/→        previous / next column
  PgUp/PgDn  scroll 10 rows
  c          column info (type, stats, format)
  ?          this help
  :close     close file and open file picker

Mouse: click table cells, column info options, or column header borders to resize; wheel on table scrolls rows; wheel on column list scrolls columns.

Press q or ? to close.";

enum MainKeyAction {
    Continue,
    Quit,
    CloseFile,
}

struct LayoutAreas {
    table: Rect,
    columns: Rect,
    column_info_popup: Option<Rect>,
    file_picker_list: Option<Rect>,
}

struct ColumnResize {
    col: usize,
    start_x: u16,
    start_width: u16,
}

pub fn run(file: Option<&str>) -> Result<()> {
    let file_path = file.map(PathBuf::from);
    let mut model = AppModel::open(file_path.clone())?;
    let mut file_picker = if FilePicker::needs_picker(&file_path) {
        Some(FilePicker::new(
            model.settings.file_picker.normalized_extensions(),
        )?)
    } else {
        None
    };

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
        column_info_popup: None,
        file_picker_list: None,
    };
    let mut last_redraw = Instant::now();
    let mut column_resize: Option<ColumnResize> = None;
    let mut command_buf: Option<String> = None;

    while running {
        model.maybe_update_column_layout();
        terminal.draw(|frame| {
            areas = if file_picker.is_some() {
                draw_file_picker(frame, file_picker.as_ref().unwrap())
            } else {
                draw(frame, &model, command_buf.as_deref())
            };
        })?;

        if file_picker.is_some() {
            let list_area = areas.file_picker_list.unwrap_or_default();
            let visible_height = Block::default()
                .borders(Borders::ALL)
                .inner(list_area)
                .height
                .max(1) as usize;

            let timeout = Duration::from_millis(50);
            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        if let Some(picker) = file_picker.as_mut() {
                            match picker.handle_key(key, visible_height) {
                                FilePickerAction::Continue => {}
                                FilePickerAction::Quit => running = false,
                                FilePickerAction::Open(path) => {
                                    model.reopen(path)?;
                                    file_picker = None;
                                }
                            }
                        }
                    }
                    Event::Mouse(mouse) if matches!(
                        mouse.kind,
                        MouseEventKind::Down(crossterm::event::MouseButton::Left)
                    ) =>
                    {
                        if let Some(picker) = file_picker.as_mut() {
                            match picker.handle_click(mouse.row, list_area) {
                                FilePickerAction::Continue => {}
                                FilePickerAction::Quit => running = false,
                                FilePickerAction::Open(path) => {
                                    model.reopen(path)?;
                                    file_picker = None;
                                }
                            }
                        }
                    }
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }
            continue;
        }

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
                    match handle_key(key, &mut model, column_list_height, &mut command_buf) {
                        MainKeyAction::Continue => {}
                        MainKeyAction::Quit => running = false,
                        MainKeyAction::CloseFile => {
                            let extensions = model.settings.file_picker.normalized_extensions();
                            let reopen_at = model.file_path.clone();
                            model.close_file()?;
                            file_picker = Some(if let Some(path) = reopen_at {
                                let dir = path
                                    .parent()
                                    .map(PathBuf::from)
                                    .unwrap_or_else(|| PathBuf::from("."));
                                FilePicker::in_dir(dir, extensions, Some(path))?
                            } else {
                                FilePicker::new(extensions)?
                            });
                            command_buf = None;
                        }
                    }
                }
                Event::Mouse(mouse) => {
                    handle_mouse(
                        mouse,
                        &mut model,
                        &areas,
                        column_list_height,
                        &mut column_resize,
                    );
                }
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

fn handle_key(
    key: KeyEvent,
    model: &mut AppModel,
    column_list_height: usize,
    command_buf: &mut Option<String>,
) -> MainKeyAction {
    if let Some(buf) = command_buf.as_mut() {
        match key.code {
            KeyCode::Esc => {
                *command_buf = None;
            }
            KeyCode::Enter => {
                let cmd = buf.trim().to_ascii_lowercase();
                *command_buf = None;
                if cmd == ":close" {
                    return MainKeyAction::CloseFile;
                }
            }
            KeyCode::Backspace => {
                buf.pop();
                if buf.is_empty() {
                    *command_buf = None;
                }
            }
            KeyCode::Char(c) if !c.is_ascii_control() => buf.push(c),
            _ => {}
        }
        return MainKeyAction::Continue;
    }

    if model.view.show_column_info {
        if model.view.column_info_decimal_editing {
            match key.code {
                KeyCode::Char('q') => model.close_column_info_pane(),
                KeyCode::Enter => model.column_info_apply_decimal_draft(),
                KeyCode::Backspace => model.column_info_decimal_backspace(),
                KeyCode::Char(c) => model.column_info_decimal_push_char(c),
                _ => {}
            }
            return MainKeyAction::Continue;
        }
        match key.code {
            KeyCode::Char('q') => model.close_column_info_pane(),
            KeyCode::Up | KeyCode::Char('k') => model.column_info_focus_delta(-1),
            KeyCode::Down | KeyCode::Char('j') => model.column_info_focus_delta(1),
            KeyCode::Enter => model.column_info_apply_focus(),
            KeyCode::Char(c) if c.is_ascii() && !c.is_ascii_control() => {
                let col = model.view.selected_col;
                if model.view.column_info_focus == model.column_info_decimal_focus_index(col) {
                    model.column_info_start_decimal_edit();
                    model.column_info_decimal_push_char(c);
                }
            }
            _ => {}
        }
        return MainKeyAction::Continue;
    }

    if model.view.show_help {
        if matches!(key.code, KeyCode::Char('?') | KeyCode::Char('q')) {
            model.view.show_help = false;
        }
        return MainKeyAction::Continue;
    }

    match key.code {
        KeyCode::Char('q') => return MainKeyAction::Quit,
        KeyCode::Char(':') => {
            *command_buf = Some(":".to_string());
        }
        KeyCode::Char('?') => model.view.show_help = true,
        KeyCode::Char('c') => model.open_column_info_pane(),
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
    MainKeyAction::Continue
}

fn handle_mouse(
    mouse: crossterm::event::MouseEvent,
    model: &mut AppModel,
    areas: &LayoutAreas,
    column_list_height: usize,
    column_resize: &mut Option<ColumnResize>,
) {
    if model.view.show_help {
        return;
    }

    if model.view.show_column_info {
        if matches!(
            mouse.kind,
            MouseEventKind::Down(crossterm::event::MouseButton::Left)
        ) {
            let pos = Position {
                x: mouse.column,
                y: mouse.row,
            };
            if let Some(popup) = areas.column_info_popup {
                let inner = Block::default().borders(Borders::ALL).inner(popup);
                if inner.contains(pos) {
                    let line = mouse.row.saturating_sub(inner.y) as usize;
                    if let Some(option) = column_info_option_at_line(
                        line,
                        model.column_info_type_kinds(model.view.selected_col).len(),
                        model.column_info_repr_section_visible(model.view.selected_col),
                    ) {
                        model.column_info_apply_option(option);
                    }
                }
            }
        }
        return;
    }

    let row = mouse.row;
    let col = mouse.column;
    let pos = Position { x: col, y: row };

    if let Some(resize) = column_resize.as_ref() {
        match mouse.kind {
            MouseEventKind::Drag(crossterm::event::MouseButton::Left)
            | MouseEventKind::Moved => {
                let delta = col as i32 - resize.start_x as i32;
                let new_width = (resize.start_width as i32 + delta)
                    .clamp(MIN_COLUMN_WIDTH as i32, MAX_COLUMN_WIDTH as i32) as u16;
                model.set_column_width(resize.col, new_width);
            }
            MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
                *column_resize = None;
            }
            _ => {}
        }
        return;
    }

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
            if let Some(resize_col) = hit_test_column_resize(col, row, areas.table, model) {
                *column_resize = Some(ColumnResize {
                    col: resize_col,
                    start_x: col,
                    start_width: model.column_width_chars(resize_col) as u16,
                });
                return;
            }
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

/// Map a screen coordinate to a column border for resize (header row only).
fn hit_test_column_resize(
    mouse_x: u16,
    mouse_y: u16,
    table_area: Rect,
    model: &AppModel,
) -> Option<usize> {
    let inner = Block::default().borders(Borders::ALL).inner(table_area);
    if mouse_y != inner.y {
        return None;
    }
    if !inner.contains(Position {
        x: mouse_x,
        y: mouse_y,
    }) {
        return None;
    }

    let col_range = model.visible_column_range(table_area.width);
    if col_range.is_empty() {
        return None;
    }

    let rel_x = mouse_x.saturating_sub(inner.x);
    let mut x = 0u16;
    for col_idx in col_range {
        let w = model.column_width_chars(col_idx) as u16;
        let right = x.saturating_add(w);
        if rel_x + 1 >= right && rel_x <= right.saturating_add(1) {
            return Some(col_idx);
        }
        x = right.saturating_add(1);
    }
    None
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
    if col_range.is_empty() {
        return None;
    }

    let rel_x = mouse_x.saturating_sub(inner.x);
    let mut x = 0u16;
    let mut col_idx = None;
    for idx in col_range.clone() {
        let w = model.column_width_chars(idx) as u16;
        let right = x.saturating_add(w);
        if rel_x < right {
            col_idx = Some(idx);
            break;
        }
        x = right.saturating_add(1);
    }
    let col_idx = col_idx?;
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

fn draw_file_picker(frame: &mut ratatui::Frame, picker: &FilePicker) -> LayoutAreas {
    let area = frame.area();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5), Constraint::Length(1)])
        .split(area);
    let visible_height = Block::default()
        .borders(Borders::ALL)
        .inner(layout[1])
        .height
        .max(1) as usize;
    picker.draw(frame, area, visible_height);
    LayoutAreas {
        table: Rect::default(),
        columns: Rect::default(),
        column_info_popup: None,
        file_picker_list: Some(layout[1]),
    }
}

fn draw(frame: &mut ratatui::Frame, model: &AppModel, command_buf: Option<&str>) -> LayoutAreas {
    let area = frame.area();
    let bottom_height = if command_buf.is_some() { 3 } else { 1 };

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(bottom_height),
        ])
        .split(area);

    let title = format!(
        " csv  │  {}  │  {} rows",
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

    if let Some(buf) = command_buf {
        frame.render_widget(
            Paragraph::new(buf).block(
                Block::default()
                    .title(" Command ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow)),
            ),
            outer[2],
        );
    } else {
        let hints = " q quit  ↑↓ rows  ←→ cols  drag resize  c info  ? help  : command ";
        frame.render_widget(
            Paragraph::new(hints).style(Style::default().fg(Color::DarkGray)),
            outer[2],
        );
    }

    if model.view.show_help {
        draw_help(frame, area);
    }
    let column_info_popup = if model.view.show_column_info {
        let popup_area = centered_rect(54, 72, area);
        draw_column_info(frame, popup_area, model);
        Some(popup_area)
    } else {
        None
    };

    LayoutAreas {
        table: table_area,
        columns: columns_area,
        column_info_popup,
        file_picker_list: None,
    }
}

fn draw_table(frame: &mut ratatui::Frame, area: Rect, model: &AppModel) {
    let headers = model.preview.headers();
    if headers.is_empty() {
        frame.render_widget(
            Paragraph::new("No data loaded.")
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
                Cell::from(model.format_column_header(col_idx, name)).style(style)
            })
            .collect::<Vec<_>>(),
    )
    .height(1)
    .bottom_margin(1);

    let body_height = area.height.saturating_sub(3) as usize;
    let mut rows = Vec::new();
    for r in 0..body_height {
        let row_idx = model.view.row_offset + r;
        let Some(fields) = model.preview.row_fields(row_idx) else {
            break;
        };
        let row_selected = row_idx == model.view.selected_row;
        let cells: Vec<Cell> = col_range
            .clone()
            .enumerate()
            .map(|(_i, col_idx)| {
                let text = fields.get(col_idx).map(String::as_str).unwrap_or("");
                let display = model.format_column_cell(col_idx, text);
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

    let widths: Vec<Constraint> = col_range
        .clone()
        .map(|col_idx| Constraint::Length(model.column_width_chars(col_idx) as u16))
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
            let text = model.format_sidebar_column_label(col_idx, name);
            let display = truncate_middle(&text, inner.width as usize);
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

fn column_info_option_at_line(
    line: usize,
    type_count: usize,
    repr_section_visible: bool,
) -> Option<usize> {
    if type_count > 0 && (3..3 + type_count).contains(&line) {
        return Some(line - 3);
    }
    if repr_section_visible {
        let repr_start = 3 + type_count + 2;
        if (repr_start..repr_start + 2).contains(&line) {
            return Some(line - repr_start + type_count);
        }
        let decimal_line = repr_start + 4;
        if line == decimal_line {
            return Some(type_count + 2);
        }
    }
    None
}

fn draw_column_info(frame: &mut ratatui::Frame, popup_area: Rect, model: &AppModel) {
    frame.render_widget(Clear, popup_area);

    let col = model.view.selected_col;
    let headers = model.preview.headers();
    let name = headers.get(col).map(String::as_str).unwrap_or("?");
    let stored = model.stored_column_kind(col);
    let effective = model.effective_column_kind(col);
    let repr = model.numeric_repr(col);
    let focus = model.view.column_info_focus;
    let type_kinds = model.column_info_type_kinds(col);
    let repr_section = model.column_info_repr_section_visible(col);
    let info = model.column_info(col);

    let mut lines = vec![Line::from(vec![Span::styled(
        format!(" Column {col}: {name} "),
        Style::default().add_modifier(Modifier::BOLD),
    )])];

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " Type ",
        Style::default().add_modifier(Modifier::BOLD),
    )));

    for (idx, kind) in type_kinds.iter().enumerate() {
        let marker = if focus == idx { "▸ " } else { "  " };
        let selected = stored == *kind;
        let check = if selected { " ✓" } else { "" };
        let mut label = kind.label().to_string();
        if *kind == ColumnKind::Auto && effective != ColumnKind::Auto {
            label = format!("{label} (inferred: {})", effective.label());
        }
        let style = if focus == idx {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else if selected {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(
            format!("{marker}{label}{check}"),
            style,
        )));
    }

    if repr_section {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " Representation ",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        for (offset, repr_opt) in [NumericRepr::General, NumericRepr::Scientific]
            .iter()
            .enumerate()
        {
            let idx = type_kinds.len() + offset;
            let marker = if focus == idx { "▸ " } else { "  " };
            let selected = repr == *repr_opt;
            let check = if selected { " ✓" } else { "" };
            let style = if focus == idx {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if selected {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            };
            lines.push(Line::from(Span::styled(
                format!("{marker}{}{check}", repr_opt.label()),
                style,
            )));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " Decimal places ",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        let decimal_idx = type_kinds.len() + 2;
        let decimal_value = if model.view.column_info_decimal_editing {
            model.view.column_info_decimal_draft.clone()
        } else {
            model.decimal_format_for_column(col).to_string()
        };
        let decimal_marker = if focus == decimal_idx { "▸ " } else { "  " };
        let decimal_style = if focus == decimal_idx || model.view.column_info_decimal_editing {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let edit_hint = if model.view.column_info_decimal_editing {
            " (Enter apply)"
        } else {
            ""
        };
        lines.push(Line::from(Span::styled(
            format!("{decimal_marker}[{decimal_value}]{edit_hint}"),
            decimal_style,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " Statistics ",
        Style::default().add_modifier(Modifier::BOLD),
    )));

    for stat in &info.stats {
        lines.push(Line::from(vec![
            Span::raw(format!("  {:<16} ", stat.label)),
            Span::styled(stat.value.clone(), Style::default().fg(Color::Yellow)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " click/↑↓  Enter apply  q close ",
        Style::default().fg(Color::DarkGray),
    )));

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(" Column info ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta)),
        ),
        popup_area,
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
