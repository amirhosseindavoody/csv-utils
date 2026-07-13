use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use csv_utils_core::column::{ColumnKind, NumericRepr};
use csv_utils_core::display::truncate_middle;
use csv_utils_core::model::{AppModel, MultiSelectAxis, MAX_COLUMN_WIDTH, MIN_COLUMN_WIDTH};
use csv_utils_core::ViewLayout;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Margin, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState,
    Table, Wrap,
};
use ratatui::Terminal;
use std::io::{self, stdout, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::web::{empty_client_view, sync_snapshot, WebServer, WebServerState};

use crate::tui::column_finder::{ColumnFinderAction, ColumnFinderState};
use crate::tui::command_line::{CommandKeyAction, CommandLineState, VIEW_COMMANDS};
use crate::tui::file_picker::{FilePicker, FilePickerAction, resolve_path};
use crate::tui::scroll::{
    horizontal_scrollbar_hit, horizontal_scrollbar_track, position_from_horizontal_track_x,
    position_from_vertical_track_y, vertical_scrollbar_hit, vertical_scrollbar_track,
    HorizontalScrollHit, ScrollMetrics, VerticalScrollHit,
};

const HELP_TEXT: &str = "\
csv — keyboard shortcuts

  q          close file (picker) or quit; closes open panels first
  ↑/↓        previous / next row (then Space toggles row multi-select)
  ←/→        previous / next column (then Space toggles column multi-select)
  Space      toggle multi-select on row or column (follows last arrow axis)
  PgUp/PgDn  scroll 10 rows
  c          column info (type, stats, format)
  r          row as JSON (floating panel; drag title, resize corner)
  PgUp/PgDn  scroll column info / row JSON panel (while open)
  ←/→        horizontal scroll in row JSON panel (while open)
  ?          this help
  :open      open file or browse directory by path
  :close     close file and open file picker
  :toggle-borders  show or hide table column border lines
  :hide / :h  hide selected columns (←/→ or sidebar) or rows (↑/↓, drag, or Ctrl+click cells)
  :unhide / :u  unhide selected or all hidden columns/rows (same axis as :hide)
  :sort        sort rows by selected column (asc → desc → clear)
  :sort asc|desc|clear  explicit sort direction or clear
  :web       open browser UI on a free local port and exit terminal view
  /          fuzzy-find columns (filters sidebar)
  p          pin/unpin selected row(s) (↑/↓ row axis) or column(s) (sidebar / ←/→)
  :filter    filter rows on selected column, or sidebar when focused (:f)

Mouse: click table cells; drag on table body to select a cell rectangle; Ctrl+click table cells to toggle individual cells; Ctrl+click column header, sidebar, or row gutter to add to selection; right-click column header, sidebar, or row gutter for context menu; drag header borders to resize columns; drag sidebar left border to resize sidebar; wheel scrolls rows/columns. Row JSON panel: drag title bar to move, drag bottom-right corner to resize, scrollbars/wheel for overflow. Click sidebar to focus it — then ↑/↓ navigate columns.

Press q or ? to close.";

enum MainKeyAction {
    Continue,
    Quit,
    CloseFile,
    OpenPath(PathBuf),
    LaunchWeb,
}

struct LayoutAreas {
    table: Rect,
    columns: Rect,
    column_info_popup: Option<Rect>,
    row_json_popup: Option<Rect>,
    file_picker_list: Option<Rect>,
}

struct ColumnResize {
    col: usize,
    start_x: u16,
    start_width: u16,
}

struct SidebarResize {
    start_x: u16,
    start_width: u16,
}

const MIN_SIDEBAR_WIDTH: u16 = 16;
const MAX_SIDEBAR_WIDTH: u16 = 80;
const MIN_TABLE_WIDTH: u16 = 20;

struct CellRangeDrag {
    anchor: (usize, usize),
}

#[derive(Debug, Clone, Copy)]
enum ScrollbarDragTarget {
    TableRows {
        viewport_rows: usize,
    },
    TableCols {
        table_width: u16,
    },
    ColumnList {
        visible_height: usize,
    },
    ColumnInfo {
        viewport: u16,
        total_lines: usize,
    },
    RowJsonVert {
        viewport_w: u16,
        viewport_h: u16,
    },
    RowJsonHoriz {
        viewport_w: u16,
        viewport_h: u16,
    },
}

struct ScrollbarDrag {
    target: ScrollbarDragTarget,
    grab_offset: u16,
}

struct FloatingPanelDrag {
    grab_x: u16,
    grab_y: u16,
}

struct FloatingPanelResize {
    start_mouse_x: u16,
    start_mouse_y: u16,
    start_w: u16,
    start_h: u16,
}

#[derive(Debug, Clone, Copy)]
enum ColumnContextAction {
    Select,
    Hide,
    Unhide,
    Info,
    TogglePin,
    SortAsc,
    SortDesc,
    ClearSort,
}

struct ColumnContextMenuItem {
    action: ColumnContextAction,
    label: String,
}

struct ColumnContextMenu {
    area: Rect,
    col: usize,
    selected: usize,
    items: Vec<ColumnContextMenuItem>,
}

impl ColumnContextMenu {
    fn for_column(col: usize, model: &AppModel, click: Position, screen: Rect) -> Self {
        let mut items = Vec::new();
        items.push(ColumnContextMenuItem {
            action: ColumnContextAction::Select,
            label: "Select".to_string(),
        });
        if model.is_column_hidden(col) {
            items.push(ColumnContextMenuItem {
                action: ColumnContextAction::Unhide,
                label: "Unhide".to_string(),
            });
        } else {
            items.push(ColumnContextMenuItem {
                action: ColumnContextAction::Hide,
                label: "Hide".to_string(),
            });
        }
        items.push(ColumnContextMenuItem {
            action: ColumnContextAction::Info,
            label: "Info".to_string(),
        });
        items.push(ColumnContextMenuItem {
            action: ColumnContextAction::TogglePin,
            label: if model.is_column_pinned(col) {
                "Unpin".to_string()
            } else {
                "Pin".to_string()
            },
        });
        items.push(ColumnContextMenuItem {
            action: ColumnContextAction::SortAsc,
            label: "Sort ascending".to_string(),
        });
        items.push(ColumnContextMenuItem {
            action: ColumnContextAction::SortDesc,
            label: "Sort descending".to_string(),
        });
        if model.is_sorted_by_column(col) {
            items.push(ColumnContextMenuItem {
                action: ColumnContextAction::ClearSort,
                label: "Clear sort".to_string(),
            });
        }
        let width = items
            .iter()
            .map(|item| item.label.len().saturating_add(2))
            .max()
            .unwrap_or(8)
            .saturating_add(2) as u16;
        let height = items.len() as u16 + 2;
        let x = click.x.min(screen.width.saturating_sub(width));
        let y = click.y.min(screen.height.saturating_sub(height));
        Self {
            area: Rect {
                x,
                y,
                width,
                height,
            },
            col,
            selected: 0,
            items,
        }
    }

    fn item_at(&self, pos: Position) -> Option<usize> {
        let inner = Block::default().borders(Borders::ALL).inner(self.area);
        if !inner.contains(pos) {
            return None;
        }
        let rel = pos.y.saturating_sub(inner.y) as usize;
        if rel < self.items.len() {
            Some(rel)
        } else {
            None
        }
    }
}

fn execute_column_context_action(
    action: ColumnContextAction,
    model: &mut AppModel,
    col: usize,
    column_list_height: usize,
) -> Option<String> {
    match action {
        ColumnContextAction::Select => {
            model.add_column_to_selection(col, column_list_height);
            None
        }
        ColumnContextAction::Hide => {
            model.focus_column_for_context_action(col, column_list_height, false);
            model.hide_selected_columns().err().map(str::to_string)
        }
        ColumnContextAction::Unhide => {
            model.focus_column_for_context_action(col, column_list_height, false);
            model.unhide_selected_columns().err().map(str::to_string)
        }
        ColumnContextAction::Info => {
            model.focus_column_for_context_action(col, column_list_height, false);
            model.open_column_info_pane();
            None
        }
        ColumnContextAction::TogglePin => {
            model.focus_column_for_context_action(col, column_list_height, false);
            model.toggle_pin_selected_columns();
            model.ensure_column_list_shows_selection(column_list_height);
            None
        }
        ColumnContextAction::SortAsc => {
            model.focus_column_for_context_action(col, column_list_height, false);
            model.set_sort_column(col, csv_utils_core::sort::SortDirection::Ascending);
            None
        }
        ColumnContextAction::SortDesc => {
            model.focus_column_for_context_action(col, column_list_height, false);
            model.set_sort_column(col, csv_utils_core::sort::SortDirection::Descending);
            None
        }
        ColumnContextAction::ClearSort => {
            model.focus_column_for_context_action(col, column_list_height, false);
            model.clear_sort();
            None
        }
    }
}

fn draw_column_context_menu(frame: &mut ratatui::Frame, menu: &ColumnContextMenu) {
    draw_context_menu(frame, menu.area, menu.selected, menu.items.iter().map(|item| item.label.as_str()));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowContextAction {
    Select,
    Hide,
    Unhide,
    TogglePin,
}

struct RowContextMenuItem {
    action: RowContextAction,
    label: String,
}

struct RowContextMenu {
    area: Rect,
    row: usize,
    selected: usize,
    items: Vec<RowContextMenuItem>,
}

impl RowContextMenu {
    fn for_row(row: usize, model: &AppModel, click: Position, screen: Rect) -> Self {
        let mut items = Vec::new();
        items.push(RowContextMenuItem {
            action: RowContextAction::Select,
            label: "Select".to_string(),
        });
        if model.is_row_hidden(row) {
            items.push(RowContextMenuItem {
                action: RowContextAction::Unhide,
                label: "Unhide".to_string(),
            });
        } else {
            items.push(RowContextMenuItem {
                action: RowContextAction::Hide,
                label: "Hide".to_string(),
            });
        }
        items.push(RowContextMenuItem {
            action: RowContextAction::TogglePin,
            label: if model.is_row_pinned(row) {
                "Unpin".to_string()
            } else {
                "Pin".to_string()
            },
        });
        let width = items
            .iter()
            .map(|item| item.label.len().saturating_add(2))
            .max()
            .unwrap_or(8)
            .saturating_add(2) as u16;
        let height = items.len() as u16 + 2;
        let x = click.x.min(screen.width.saturating_sub(width));
        let y = click.y.min(screen.height.saturating_sub(height));
        Self {
            area: Rect {
                x,
                y,
                width,
                height,
            },
            row,
            selected: 0,
            items,
        }
    }

    fn item_at(&self, pos: Position) -> Option<usize> {
        let inner = Block::default().borders(Borders::ALL).inner(self.area);
        if !inner.contains(pos) {
            return None;
        }
        let rel = pos.y.saturating_sub(inner.y) as usize;
        if rel < self.items.len() {
            Some(rel)
        } else {
            None
        }
    }
}

fn execute_row_context_action(
    action: RowContextAction,
    model: &mut AppModel,
    row: usize,
) -> Option<String> {
    match action {
        RowContextAction::Select => {
            model.add_row_to_selection(row);
            None
        }
        RowContextAction::Hide => {
            model.focus_row_for_context_action(row, false);
            model.hide_selected_rows().err().map(str::to_string)
        }
        RowContextAction::Unhide => {
            model.focus_row_for_context_action(row, false);
            model.unhide_selected_rows().err().map(str::to_string)
        }
        RowContextAction::TogglePin => {
            model.focus_row_for_context_action(row, false);
            model.toggle_pin_selected_rows();
            None
        }
    }
}

fn draw_context_menu<'a>(
    frame: &mut ratatui::Frame,
    area: Rect,
    selected: usize,
    labels: impl Iterator<Item = &'a str>,
) {
    let mut lines = Vec::new();
    for (idx, label) in labels.enumerate() {
        let style = if idx == selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(format!(" {} ", label), style)));
    }
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        area,
    );
}

fn draw_row_context_menu(frame: &mut ratatui::Frame, menu: &RowContextMenu) {
    draw_context_menu(frame, menu.area, menu.selected, menu.items.iter().map(|item| item.label.as_str()));
}

pub fn run(file: Option<&str>) -> Result<()> {
    let file_path = file.map(PathBuf::from);
    let shared_model = Arc::new(Mutex::new(AppModel::open(file_path.clone())?));
    let web_layout = Arc::new(Mutex::new(ViewLayout::default()));
    let web_state = WebServerState {
        model: Arc::clone(&shared_model),
        layout: Arc::clone(&web_layout),
        snapshot: Arc::new(std::sync::RwLock::new(empty_client_view())),
    };
    let mut web_server: Option<WebServer> = None;
    let mut file_picker = if FilePicker::needs_picker(&file_path) {
        Some(FilePicker::new(
            shared_model
                .lock()
                .unwrap()
                .settings
                .file_picker
                .normalized_extensions(),
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
        row_json_popup: None,
        file_picker_list: None,
    };
    let mut last_redraw = Instant::now();
    let mut last_scan_row_count = 0usize;
    let mut last_scan_done = true;
    let mut last_scan_error = false;
    let mut needs_redraw = true;
    let mut column_resize: Option<ColumnResize> = None;
    let mut sidebar_resize: Option<SidebarResize> = None;
    let mut cell_range_drag: Option<CellRangeDrag> = None;
    let mut scrollbar_drag: Option<ScrollbarDrag> = None;
    let mut row_json_drag: Option<FloatingPanelDrag> = None;
    let mut row_json_resize: Option<FloatingPanelResize> = None;
    let mut command_line: Option<CommandLineState> = None;
    let mut command_error: Option<String> = None;
    let mut column_finder: Option<ColumnFinderState> = None;
    let mut column_context_menu: Option<ColumnContextMenu> = None;
    let mut row_context_menu: Option<RowContextMenu> = None;
    let mut pending_web_layout: Option<ViewLayout> = None;

    const SCAN_REDRAW_INTERVAL: Duration = Duration::from_millis(100);
    const IDLE_POLL_TIMEOUT: Duration = Duration::from_millis(50);
    const SCAN_POLL_TIMEOUT: Duration = Duration::from_millis(50);

    while running {
        if needs_redraw {
            let mut model = shared_model.lock().unwrap();
            model.maybe_update_column_layout();
            terminal.draw(|frame| {
                areas = if file_picker.is_some() {
                    draw_file_picker(frame, file_picker.as_ref().unwrap())
                } else {
                    draw(
                        frame,
                        &mut model,
                        command_line.as_ref(),
                        command_error.as_deref(),
                        column_finder.as_ref(),
                        column_context_menu.as_ref(),
                        row_context_menu.as_ref(),
                    )
                };
            })?;

            if file_picker.is_none() {
                let table_width = areas.table.width;
                let viewport_rows = areas.table.height.saturating_sub(3) as usize;
                let column_list_height = column_list_visible_height(areas.columns);
                model.clamp_selection(viewport_rows.max(1), table_width);
                model.clamp_column_list_offset(column_list_height);
            }

            last_scan_row_count = model.preview.row_count();
            last_scan_done = model.preview.scan_done();
            last_scan_error = model.preview.scan_error();
            last_redraw = Instant::now();
            needs_redraw = false;
        }

        if file_picker.is_some() {
            let list_area = areas.file_picker_list.unwrap_or_default();
            let visible_height = Block::default()
                .borders(Borders::ALL)
                .inner(list_area)
                .height
                .max(1) as usize;

            let timeout = IDLE_POLL_TIMEOUT;
            if event::poll(timeout)? {
                let mut model = shared_model.lock().unwrap();
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        if let Some(picker) = file_picker.as_mut() {
                            match picker.handle_key(key, visible_height) {
                                FilePickerAction::Continue => {
                                    needs_redraw = true;
                                }
                                FilePickerAction::Quit => running = false,
                                FilePickerAction::Open(path) => {
                                    model.reopen(path)?;
                                    file_picker = None;
                                    needs_redraw = true;
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
                                FilePickerAction::Continue => {
                                    needs_redraw = true;
                                }
                                FilePickerAction::Quit => running = false,
                                FilePickerAction::Open(path) => {
                                    model.reopen(path)?;
                                    file_picker = None;
                                    needs_redraw = true;
                                }
                            }
                        }
                    }
                    Event::Resize(_, _) => {
                        needs_redraw = true;
                    }
                    _ => {}
                }
            }
            continue;
        }

        let size = terminal.size()?;
        let table_width = areas.table.width;
        let viewport_rows = areas.table.height.saturating_sub(3) as usize;
        let column_list_height = column_list_visible_height(areas.columns);

        let scan_active = {
            let model = shared_model.lock().unwrap();
            !model.preview.scan_done()
        };
        let timeout = if scan_active {
            SCAN_POLL_TIMEOUT
        } else {
            IDLE_POLL_TIMEOUT
        };

        if event::poll(timeout)? {
            let mut model = shared_model.lock().unwrap();
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    match handle_key(
                        key,
                        &mut model,
                        column_list_height,
                        areas.column_info_popup,
                        areas.row_json_popup,
                        &mut command_line,
                        &mut command_error,
                        &mut column_finder,
                        &mut column_context_menu,
                        &mut row_context_menu,
                    ) {
                        MainKeyAction::Continue => {
                            needs_redraw = true;
                        }
                        MainKeyAction::Quit => running = false,
                        MainKeyAction::LaunchWeb => {
                            pending_web_layout = Some(ViewLayout {
                                viewport_rows: viewport_rows.max(1),
                                table_width,
                                column_list_height,
                            });
                            running = false;
                        }
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
                            command_line = None;
                            command_error = None;
                            column_finder = None;
                            column_context_menu = None;
                            row_context_menu = None;
                            needs_redraw = true;
                        }
                        MainKeyAction::OpenPath(path) => {
                            let extensions = model.settings.file_picker.normalized_extensions();
                            if path.is_dir() {
                                model.close_file()?;
                                file_picker = Some(FilePicker::in_dir(path, extensions, None)?);
                            } else if path.is_file() {
                                model.reopen(path)?;
                            } else {
                                command_error =
                                    Some(format!("Path not found: {}", path.display()));
                                needs_redraw = true;
                                continue;
                            }
                            command_line = None;
                            command_error = None;
                            column_finder = None;
                            column_context_menu = None;
                            row_context_menu = None;
                            needs_redraw = true;
                        }
                    }
                }
                Event::Mouse(mouse) => {
                    let screen = Rect::new(0, 0, size.width, size.height);
                    handle_mouse(
                        mouse,
                        &mut model,
                        &areas,
                        column_list_height,
                        &mut column_resize,
                        &mut sidebar_resize,
                        &mut cell_range_drag,
                        &mut scrollbar_drag,
                        &mut row_json_drag,
                        &mut row_json_resize,
                        &mut column_context_menu,
                        &mut row_context_menu,
                        &mut command_error,
                        screen,
                    );
                    needs_redraw = true;
                }
                Event::Resize(_, _) => {
                    needs_redraw = true;
                }
                _ => {}
            }
        } else if scan_active {
            let model = shared_model.lock().unwrap();
            let row_count = model.preview.row_count();
            let scan_done = model.preview.scan_done();
            let scan_error = model.preview.scan_error();
            let progress_changed = row_count != last_scan_row_count
                || scan_done != last_scan_done
                || scan_error != last_scan_error;
            if progress_changed && last_redraw.elapsed() >= SCAN_REDRAW_INTERVAL {
                needs_redraw = true;
            }
        }

        if let Some(layout) = pending_web_layout.take() {
            if let Ok(mut web_l) = web_layout.lock() {
                *web_l = layout;
            }
            sync_snapshot(&web_state);
            web_server = Some(WebServer::start(web_state.clone())?);
            break;
        }
    }

    if web_server.is_none() {
        shared_model.lock().unwrap().abandon_scan_thread();
    }

    restore_terminal(&mut terminal)?;

    if let Some(server) = web_server.take() {
        eprintln!("Web UI at {}", server.url());
        eprintln!("Press Ctrl+C to stop.");
        let _ = io::stderr().flush();
        server.wait();
        shared_model.lock().unwrap().abandon_scan_thread();
    }

    // Issue #24: return to the shell immediately. `close_file`/`reopen` already
    // offload heavy `AppModel` drops to detached threads (`replace_and_discard`),
    // but the process can still linger while those threads free millions of
    // per-column stats allocations — and after `:web` the model may still hold
    // that state when the session ends. Exiting here avoids blocking the prompt
    // on deallocation or platform-specific waits for spawned threads.
    std::process::exit(0);
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    terminal
        .backend_mut()
        .execute(crossterm::event::DisableMouseCapture)?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    Ok(())
}

fn column_list_visible_height(columns_area: Rect) -> usize {
    Block::default()
        .borders(Borders::ALL)
        .inner(columns_area)
        .height
        .max(1) as usize
}

fn table_row_scroll_metrics(model: &AppModel, area: Rect) -> ScrollMetrics {
    let viewport = area.height.saturating_sub(3) as usize;
    let empty: &[usize] = &[];
    let matching = model.cached_matching_rows().unwrap_or(empty);
    ScrollMetrics {
        content_length: model.scrollable_table_rows(matching).len(),
        viewport_length: model.scrollable_row_visible_count(viewport),
        position: model.view.row_offset,
    }
}

fn table_col_scroll_metrics(model: &AppModel, area: Rect) -> ScrollMetrics {
    let table_width = table_data_width(model, area);
    // Use the stable fit-from-start count (same basis as max_col_offset) so the
    // thumb size does not jitter as variable-width columns enter/leave view.
    ScrollMetrics {
        content_length: model.scrollable_table_columns().len(),
        viewport_length: model.scrollable_visible_count(table_width).max(1),
        position: model.view.col_offset,
    }
}

fn column_list_scroll_metrics(model: &AppModel, area: Rect) -> ScrollMetrics {
    ScrollMetrics {
        content_length: model.filtered_sidebar_columns().len(),
        viewport_length: column_list_visible_height(area),
        position: model.view.column_list_offset,
    }
}

fn apply_scroll_position(model: &mut AppModel, target: ScrollbarDragTarget, position: usize) {
    match target {
        ScrollbarDragTarget::TableRows { viewport_rows } => {
            model.set_row_offset(position, viewport_rows)
        }
        ScrollbarDragTarget::TableCols { table_width } => {
            model.set_col_offset(position, table_width)
        }
        ScrollbarDragTarget::ColumnList { visible_height } => {
            model.set_column_list_scroll(position, visible_height)
        }
        ScrollbarDragTarget::ColumnInfo {
            viewport,
            total_lines,
        } => model.set_column_info_scroll_position(position, viewport, total_lines),
        ScrollbarDragTarget::RowJsonVert {
            viewport_w,
            viewport_h,
        } => {
            model.set_row_json_scroll(
                model.view.row_json_scroll_x as usize,
                position,
                viewport_w,
                viewport_h,
            )
        }
        ScrollbarDragTarget::RowJsonHoriz {
            viewport_w,
            viewport_h,
        } => {
            model.set_row_json_scroll(
                position,
                model.view.row_json_scroll_y as usize,
                viewport_w,
                viewport_h,
            )
        }
    }
}

fn handle_vertical_scroll_hit(
    kind: MouseEventKind,
    hit: VerticalScrollHit,
    target: ScrollbarDragTarget,
    area: Rect,
    metrics: ScrollMetrics,
    model: &mut AppModel,
    scrollbar_drag: &mut Option<ScrollbarDrag>,
) {
    match kind {
        MouseEventKind::ScrollUp => {
            apply_scroll_position(model, target, metrics.position.saturating_sub(3));
        }
        MouseEventKind::ScrollDown => {
            apply_scroll_position(
                model,
                target,
                (metrics.position + 3).min(metrics.max_position()),
            );
        }
        MouseEventKind::Down(crossterm::event::MouseButton::Left) => match hit {
            VerticalScrollHit::PageUp => {
                apply_scroll_position(
                    model,
                    target,
                    metrics.position.saturating_sub(metrics.viewport_length.max(1)),
                );
            }
            VerticalScrollHit::PageDown => {
                apply_scroll_position(
                    model,
                    target,
                    (metrics.position + metrics.viewport_length.max(1))
                        .min(metrics.max_position()),
                );
            }
            VerticalScrollHit::Thumb { grab_offset } => {
                *scrollbar_drag = Some(ScrollbarDrag {
                    target,
                    grab_offset,
                });
            }
            VerticalScrollHit::Track { rel_y } => {
                let track = vertical_scrollbar_track(area);
                let (_, thumb_len) =
                    super::scroll::vertical_thumb_bounds(track.height as usize, metrics);
                let grab_offset = (thumb_len / 2) as u16;
                let thumb_top = rel_y.saturating_sub(grab_offset);
                let position =
                    position_from_vertical_track_y(thumb_top, track.height, metrics);
                apply_scroll_position(model, target, position);
                *scrollbar_drag = Some(ScrollbarDrag {
                    target,
                    grab_offset,
                });
            }
        },
        _ => {}
    }
}

fn handle_horizontal_scroll_hit(
    kind: MouseEventKind,
    hit: HorizontalScrollHit,
    target: ScrollbarDragTarget,
    area: Rect,
    metrics: ScrollMetrics,
    model: &mut AppModel,
    scrollbar_drag: &mut Option<ScrollbarDrag>,
) {
    match kind {
        MouseEventKind::ScrollUp => {
            apply_scroll_position(model, target, metrics.position.saturating_sub(3));
        }
        MouseEventKind::ScrollDown => {
            apply_scroll_position(
                model,
                target,
                (metrics.position + 3).min(metrics.max_position()),
            );
        }
        MouseEventKind::Down(crossterm::event::MouseButton::Left) => match hit {
            HorizontalScrollHit::PageLeft => {
                apply_scroll_position(
                    model,
                    target,
                    metrics.position.saturating_sub(metrics.viewport_length.max(1)),
                );
            }
            HorizontalScrollHit::PageRight => {
                apply_scroll_position(
                    model,
                    target,
                    (metrics.position + metrics.viewport_length.max(1))
                        .min(metrics.max_position()),
                );
            }
            HorizontalScrollHit::Thumb { grab_offset } => {
                *scrollbar_drag = Some(ScrollbarDrag {
                    target,
                    grab_offset,
                });
            }
            HorizontalScrollHit::Track { rel_x } => {
                let track = horizontal_scrollbar_track(area);
                let (_, thumb_len) =
                    super::scroll::horizontal_thumb_bounds(track.width as usize, metrics);
                let grab_offset = (thumb_len / 2) as u16;
                let thumb_left = rel_x.saturating_sub(grab_offset);
                let position =
                    position_from_horizontal_track_x(thumb_left, track.width, metrics);
                apply_scroll_position(model, target, position);
                *scrollbar_drag = Some(ScrollbarDrag {
                    target,
                    grab_offset,
                });
            }
        },
        _ => {}
    }
}

fn apply_scrollbar_drag(
    model: &mut AppModel,
    drag: &ScrollbarDrag,
    areas: &LayoutAreas,
    pos: Position,
) {
    match drag.target {
        ScrollbarDragTarget::TableRows { viewport_rows, .. } => {
            let metrics = table_row_scroll_metrics(model, areas.table);
            let track = vertical_scrollbar_track(areas.table);
            let rel_y = pos.y.saturating_sub(track.y).saturating_sub(drag.grab_offset);
            let position = position_from_vertical_track_y(rel_y, track.height, metrics);
            model.set_row_offset(position, viewport_rows);
        }
        ScrollbarDragTarget::TableCols { table_width } => {
            let metrics = table_col_scroll_metrics(model, areas.table);
            let track = horizontal_scrollbar_track(areas.table);
            let rel_x = pos.x.saturating_sub(track.x).saturating_sub(drag.grab_offset);
            let position = position_from_horizontal_track_x(rel_x, track.width, metrics);
            model.set_col_offset(position, table_width);
        }
        ScrollbarDragTarget::ColumnList { visible_height } => {
            let metrics = column_list_scroll_metrics(model, areas.columns);
            let track = vertical_scrollbar_track(areas.columns);
            let rel_y = pos.y.saturating_sub(track.y).saturating_sub(drag.grab_offset);
            let position = position_from_vertical_track_y(rel_y, track.height, metrics);
            model.set_column_list_scroll(position, visible_height);
        }
        ScrollbarDragTarget::ColumnInfo {
            viewport,
            total_lines,
        } => {
            let metrics = ScrollMetrics {
                content_length: total_lines,
                viewport_length: viewport as usize,
                position: model.view.column_info_scroll as usize,
            };
            let popup = areas.column_info_popup.unwrap_or(areas.table);
            let track = vertical_scrollbar_track(popup);
            let rel_y = pos.y.saturating_sub(track.y).saturating_sub(drag.grab_offset);
            let position = position_from_vertical_track_y(rel_y, track.height, metrics);
            model.set_column_info_scroll_position(position, viewport, total_lines);
        }
        ScrollbarDragTarget::RowJsonVert {
            viewport_w,
            viewport_h,
        } => {
            let popup = areas.row_json_popup.unwrap_or(areas.table);
            let (content_h, _) = model.row_json_content_size();
            let metrics = ScrollMetrics {
                content_length: content_h,
                viewport_length: viewport_h as usize,
                position: model.view.row_json_scroll_y as usize,
            };
            let track = vertical_scrollbar_track(popup);
            let rel_y = pos.y.saturating_sub(track.y).saturating_sub(drag.grab_offset);
            let position = position_from_vertical_track_y(rel_y, track.height, metrics);
            model.set_row_json_scroll(
                model.view.row_json_scroll_x as usize,
                position,
                viewport_w,
                viewport_h,
            );
        }
        ScrollbarDragTarget::RowJsonHoriz {
            viewport_w,
            viewport_h,
        } => {
            let popup = areas.row_json_popup.unwrap_or(areas.table);
            let (_, content_w) = model.row_json_content_size();
            let metrics = ScrollMetrics {
                content_length: content_w,
                viewport_length: viewport_w as usize,
                position: model.view.row_json_scroll_x as usize,
            };
            let track = horizontal_scrollbar_track(popup);
            let rel_x = pos.x.saturating_sub(track.x).saturating_sub(drag.grab_offset);
            let position = position_from_horizontal_track_x(rel_x, track.width, metrics);
            model.set_row_json_scroll(
                position,
                model.view.row_json_scroll_y as usize,
                viewport_w,
                viewport_h,
            );
        }
    }
}

fn apply_column_name_filter_from_command(model: &mut AppModel, cmd: &str) -> Result<(), String> {
    if cmd == ":filter" || cmd == ":f" {
        model.clear_column_name_filter();
        return Ok(());
    }
    if let Some(query) = cmd.strip_prefix(":filter ") {
        model.set_column_name_filter(query.trim().to_string());
        return Ok(());
    }
    if let Some(query) = cmd.strip_prefix(":f ") {
        model.set_column_name_filter(query.trim().to_string());
        return Ok(());
    }
    Err(format!("Unknown command: {cmd}"))
}

fn apply_row_value_filter_from_command(
    model: &mut AppModel,
    cmd: &str,
) -> Result<(), String> {
    let col = model.view.selected_col;
    if cmd == ":filter" || cmd == ":f" {
        model.clear_column_value_filter(col);
        return Ok(());
    }
    if let Some(query) = cmd.strip_prefix(":filter ") {
        model
            .set_column_value_filter(col, query.trim().to_string())
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    if let Some(query) = cmd.strip_prefix(":f ") {
        model
            .set_column_value_filter(col, query.trim().to_string())
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    Err(format!("Unknown command: {cmd}"))
}

fn apply_filter_from_command(
    model: &mut AppModel,
    cmd: &str,
    sidebar_focused: bool,
) -> Result<(), String> {
    if sidebar_focused {
        apply_column_name_filter_from_command(model, cmd)
    } else {
        apply_row_value_filter_from_command(model, cmd)
    }
}

fn handle_key(
    key: KeyEvent,
    model: &mut AppModel,
    column_list_height: usize,
    column_info_popup: Option<Rect>,
    row_json_popup: Option<Rect>,
    command_line: &mut Option<CommandLineState>,
    command_error: &mut Option<String>,
    column_finder: &mut Option<ColumnFinderState>,
    column_context_menu: &mut Option<ColumnContextMenu>,
    row_context_menu: &mut Option<RowContextMenu>,
) -> MainKeyAction {
    if let Some(menu) = column_context_menu.as_mut() {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => *column_context_menu = None,
            KeyCode::Up | KeyCode::Char('k') => {
                menu.selected = menu.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !menu.items.is_empty() {
                    menu.selected = (menu.selected + 1).min(menu.items.len() - 1);
                }
            }
            KeyCode::Enter => {
                let action = menu.items.get(menu.selected).map(|item| item.action);
                let col = menu.col;
                *column_context_menu = None;
                if let Some(action) = action {
                    if let Some(err) =
                        execute_column_context_action(action, model, col, column_list_height)
                    {
                        *command_error = Some(err);
                    }
                }
            }
            _ => {}
        }
        return MainKeyAction::Continue;
    }

    if let Some(menu) = row_context_menu.as_mut() {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => *row_context_menu = None,
            KeyCode::Up | KeyCode::Char('k') => {
                menu.selected = menu.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !menu.items.is_empty() {
                    menu.selected = (menu.selected + 1).min(menu.items.len() - 1);
                }
            }
            KeyCode::Enter => {
                let action = menu.items.get(menu.selected).map(|item| item.action);
                let row = menu.row;
                *row_context_menu = None;
                if let Some(action) = action {
                    if let Some(err) = execute_row_context_action(action, model, row) {
                        *command_error = Some(err);
                    }
                }
            }
            _ => {}
        }
        return MainKeyAction::Continue;
    }

    if let Some(finder) = column_finder.as_mut() {
        match finder.handle_key(key, model, column_list_height) {
            ColumnFinderAction::Continue => {}
            ColumnFinderAction::Cancel => {
                model.clear_column_name_filter();
                model.view.column_sidebar_focused = false;
                *column_finder = None;
            }
            ColumnFinderAction::Select(_) => {
                model.view.column_sidebar_focused = false;
                *column_finder = None;
            }
        }
        return MainKeyAction::Continue;
    }

    if let Some(command) = command_line.as_mut() {
        match command.handle_key(key, VIEW_COMMANDS) {
            CommandKeyAction::Continue => {
                *command_error = None;
            }
            CommandKeyAction::Cancel => {
                *command_line = None;
                *command_error = None;
            }
            CommandKeyAction::Rejected => {
                if command.buf == ":filter " || command.buf == ":f " {
                    if model.view.column_sidebar_focused {
                        model.clear_column_name_filter();
                    } else {
                        model.clear_column_value_filter(model.view.selected_col);
                    }
                    *command_line = None;
                    *command_error = None;
                    model.ensure_column_list_shows_selection(column_list_height);
                } else {
                    *command_error = Some(
                        command
                            .rejection_message(VIEW_COMMANDS)
                            .to_string(),
                    );
                }
            }
            CommandKeyAction::Submit(cmd) => {
                if let Some(path_str) = cmd.strip_prefix(":open ") {
                    let path_str = path_str.trim();
                    if path_str.is_empty() {
                        *command_error = Some(":open requires a path".to_string());
                        return MainKeyAction::Continue;
                    }
                    let base = model
                        .file_path
                        .as_ref()
                        .and_then(|p| p.parent())
                        .filter(|p| !p.as_os_str().is_empty());
                    match resolve_path(path_str, base) {
                        Ok(path) => {
                            *command_line = None;
                            *command_error = None;
                            return MainKeyAction::OpenPath(path);
                        }
                        Err(err) => {
                            *command_error = Some(format!("Invalid path: {err}"));
                        }
                    }
                } else if cmd.starts_with(":filter ") || cmd.starts_with(":f ") || cmd == ":filter" || cmd == ":f" {
                    match apply_filter_from_command(
                        model,
                        &cmd,
                        model.view.column_sidebar_focused,
                    ) {
                        Ok(()) => {
                            *command_line = None;
                            *command_error = None;
                            model.ensure_column_list_shows_selection(column_list_height);
                        }
                        Err(err) => *command_error = Some(err),
                    }
                } else if cmd.starts_with(":sort ") {
                    let args = cmd.strip_prefix(":sort ").unwrap_or("");
                    match model.sort_column_from_command(args) {
                        Ok(()) => {
                            *command_line = None;
                            *command_error = None;
                        }
                        Err(msg) => *command_error = Some(msg.to_string()),
                    }
                } else {
                    match cmd.as_str() {
                        ":close" => {
                            *command_line = None;
                            *command_error = None;
                            return MainKeyAction::CloseFile;
                        }
                        ":toggle-borders" => {
                            model.toggle_column_borders();
                            *command_line = None;
                            *command_error = None;
                        }
                        ":hide" | ":h" => {
                            match model.hide_from_command() {
                                Ok(()) => {
                                    *command_line = None;
                                    *command_error = None;
                                }
                                Err(msg) => *command_error = Some(msg.to_string()),
                            }
                        }
                        ":unhide" | ":u" => {
                            match model.unhide_from_command() {
                                Ok(()) => {
                                    *command_line = None;
                                    *command_error = None;
                                }
                                Err(msg) => *command_error = Some(msg.to_string()),
                            }
                        }
                        ":sort" => {
                            match model.sort_column_from_command("") {
                                Ok(()) => {
                                    *command_line = None;
                                    *command_error = None;
                                }
                                Err(msg) => *command_error = Some(msg.to_string()),
                            }
                        }
                        ":web" => {
                            *command_line = None;
                            *command_error = None;
                            return MainKeyAction::LaunchWeb;
                        }
                        _ => *command_error = Some(format!("Unknown command: {cmd}")),
                    }
                }
            }
        }
        return MainKeyAction::Continue;
    }

    if model.view.show_row_json {
        let (viewport_w, viewport_h) = row_json_popup
            .map(row_json_viewport_size)
            .unwrap_or((40, 12));
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('r') => model.close_row_json_pane(),
            KeyCode::Up | KeyCode::Char('k') => {
                model.row_json_scroll_by(0, -1, viewport_w, viewport_h)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                model.row_json_scroll_by(0, 1, viewport_w, viewport_h)
            }
            KeyCode::Left | KeyCode::Char('h') => {
                model.row_json_scroll_by(-1, 0, viewport_w, viewport_h)
            }
            KeyCode::Right | KeyCode::Char('l') => {
                model.row_json_scroll_by(1, 0, viewport_w, viewport_h)
            }
            KeyCode::PageUp => {
                model.row_json_scroll_by(0, -(viewport_h as i32), viewport_w, viewport_h)
            }
            KeyCode::PageDown => {
                model.row_json_scroll_by(0, viewport_h as i32, viewport_w, viewport_h)
            }
            KeyCode::Home => model.set_row_json_scroll(0, 0, viewport_w, viewport_h),
            KeyCode::End => {
                let (content_h, _) = model.row_json_content_size();
                let max_y = content_h.saturating_sub(viewport_h as usize);
                model.set_row_json_scroll(
                    model.view.row_json_scroll_x as usize,
                    max_y,
                    viewport_w,
                    viewport_h,
                );
            }
            _ => {}
        }
        return MainKeyAction::Continue;
    }

    if model.view.show_column_info {
        if model.view.column_info_filter_editing {
            match key.code {
                KeyCode::Char('q') => model.close_column_info_pane(),
                KeyCode::Enter => {
                    let _ = model.column_info_apply_filter_draft();
                }
                KeyCode::Backspace => model.column_info_filter_backspace(),
                KeyCode::Char(c) => model.column_info_filter_push_char(c),
                _ => {}
            }
            return MainKeyAction::Continue;
        }
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
            KeyCode::Up | KeyCode::Char('k') => {
                model.column_info_focus_delta(-1);
                if let Some(popup) = column_info_popup {
                    let col = model.view.selected_col;
                    let viewport = column_info_viewport_lines(popup);
                    model.column_info_ensure_focus_visible(col, viewport);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                model.column_info_focus_delta(1);
                if let Some(popup) = column_info_popup {
                    let col = model.view.selected_col;
                    let viewport = column_info_viewport_lines(popup);
                    model.column_info_ensure_focus_visible(col, viewport);
                }
            }
            KeyCode::PageUp => {
                if let Some(popup) = column_info_popup {
                    let viewport = column_info_viewport_lines(popup) as i32;
                    model.column_info_scroll_by(-viewport);
                }
            }
            KeyCode::PageDown => {
                if let Some(popup) = column_info_popup {
                    let viewport = column_info_viewport_lines(popup) as i32;
                    model.column_info_scroll_by(viewport);
                }
            }
            KeyCode::Enter => model.column_info_apply_focus(),
            KeyCode::Char(c) if c.is_ascii() && !c.is_ascii_control() => {
                let col = model.view.selected_col;
                if model.view.column_info_focus == model.column_info_filter_focus_index(col) {
                    model.column_info_start_filter_edit();
                    model.column_info_filter_push_char(c);
                } else if model.view.column_info_focus
                    == model.column_info_decimal_focus_index(col)
                {
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
        KeyCode::Char('q') => {
            if model.file_path.is_some() {
                return MainKeyAction::CloseFile;
            }
            return MainKeyAction::Quit;
        }
        KeyCode::Char(':') => {
            *column_context_menu = None;
            *row_context_menu = None;
            *command_line = Some(CommandLineState::start());
            *command_error = None;
            *column_finder = None;
        }
        KeyCode::Char('/') => {
            *column_context_menu = None;
            *row_context_menu = None;
            *column_finder = Some(ColumnFinderState::start());
            *command_line = None;
            *command_error = None;
            model.view.column_sidebar_focused = true;
            column_finder.as_mut().unwrap().sync_filter(model);
        }
        KeyCode::Char('?') => model.view.show_help = true,
        KeyCode::Char('c') => model.open_column_info_pane(),
        KeyCode::Char('r') => model.open_row_json_pane(),
        KeyCode::Char('p') if model.view.column_sidebar_focused
            || model.view.last_multi_select_axis == MultiSelectAxis::Column =>
        {
            model.toggle_pin_selected_columns();
            model.ensure_column_list_shows_selection(column_list_height);
        }
        KeyCode::Char('p') if model.view.last_multi_select_axis == MultiSelectAxis::Row =>
        {
            model.toggle_pin_selected_rows();
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if model.view.column_sidebar_focused {
                model.move_selected_sidebar_column(-1, column_list_height);
            } else {
                model.view.column_sidebar_focused = false;
                model.set_multi_select_axis(MultiSelectAxis::Row);
                model.move_selected_row(-1);
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if model.view.column_sidebar_focused {
                model.move_selected_sidebar_column(1, column_list_height);
            } else {
                model.view.column_sidebar_focused = false;
                model.set_multi_select_axis(MultiSelectAxis::Row);
                model.move_selected_row(1);
            }
        }
        KeyCode::Left | KeyCode::Char('h') => {
            model.view.column_sidebar_focused = false;
            model.set_multi_select_axis(MultiSelectAxis::Column);
            model.move_selected_column(-1, column_list_height);
        }
        KeyCode::Right | KeyCode::Char('l') => {
            model.view.column_sidebar_focused = false;
            model.set_multi_select_axis(MultiSelectAxis::Column);
            model.move_selected_column(1, column_list_height);
        }
        KeyCode::PageUp => {
            model.view.column_sidebar_focused = false;
            model.set_multi_select_axis(MultiSelectAxis::Row);
            model.move_selected_row(-10);
        }
        KeyCode::PageDown => {
            model.view.column_sidebar_focused = false;
            model.set_multi_select_axis(MultiSelectAxis::Row);
            model.move_selected_row(10);
        }
        KeyCode::Home => {
            model.view.column_sidebar_focused = false;
            model.set_multi_select_axis(MultiSelectAxis::Row);
            if let Some(r) = model.first_navigation_row() {
                model.view.selected_row = r;
            }
        }
        KeyCode::End => {
            model.view.column_sidebar_focused = false;
            model.set_multi_select_axis(MultiSelectAxis::Row);
            if let Some(r) = model.last_navigation_row() {
                model.view.selected_row = r;
            }
        }
        KeyCode::Char(' ') => {
            model.view.column_sidebar_focused = false;
            model.toggle_multi_select_at_focus();
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
    sidebar_resize: &mut Option<SidebarResize>,
    cell_range_drag: &mut Option<CellRangeDrag>,
    scrollbar_drag: &mut Option<ScrollbarDrag>,
    row_json_drag: &mut Option<FloatingPanelDrag>,
    row_json_resize: &mut Option<FloatingPanelResize>,
    column_context_menu: &mut Option<ColumnContextMenu>,
    row_context_menu: &mut Option<RowContextMenu>,
    command_error: &mut Option<String>,
    screen: Rect,
) {
    if model.view.show_help {
        return;
    }

    let row = mouse.row;
    let col = mouse.column;
    let pos = Position { x: col, y: row };

    if let Some(drag) = row_json_drag.as_ref() {
        match mouse.kind {
            MouseEventKind::Drag(crossterm::event::MouseButton::Left)
            | MouseEventKind::Moved => {
                if let Some(popup) = areas.row_json_popup {
                    let width = popup.width;
                    let height = popup.height;
                    let max_x = screen.width.saturating_sub(width);
                    let max_y = screen.height.saturating_sub(height);
                    let x = col.saturating_sub(drag.grab_x).min(max_x);
                    let y = row.saturating_sub(drag.grab_y).min(max_y);
                    model.set_row_json_geometry(x, y, width, height);
                }
            }
            MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
                *row_json_drag = None;
            }
            _ => {}
        }
        return;
    }

    if let Some(resize) = row_json_resize.as_ref() {
        match mouse.kind {
            MouseEventKind::Drag(crossterm::event::MouseButton::Left)
            | MouseEventKind::Moved => {
                if let Some(popup) = areas.row_json_popup {
                    let dx = col as i32 - resize.start_mouse_x as i32;
                    let dy = row as i32 - resize.start_mouse_y as i32;
                    let width = (resize.start_w as i32 + dx)
                        .clamp(
                            AppModel::MIN_ROW_JSON_WIDTH as i32,
                            screen.width.saturating_sub(popup.x) as i32,
                        ) as u16;
                    let height = (resize.start_h as i32 + dy)
                        .clamp(
                            AppModel::MIN_ROW_JSON_HEIGHT as i32,
                            screen.height.saturating_sub(popup.y) as i32,
                        ) as u16;
                    model.set_row_json_geometry(popup.x, popup.y, width, height);
                    let (viewport_w, viewport_h) = row_json_viewport_size(Rect {
                        x: popup.x,
                        y: popup.y,
                        width,
                        height,
                    });
                    model.clamp_row_json_scroll(viewport_w, viewport_h);
                }
            }
            MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
                *row_json_resize = None;
            }
            _ => {}
        }
        return;
    }

    if let Some(menu) = column_context_menu.as_ref() {
        match mouse.kind {
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                if let Some(idx) = menu.item_at(pos) {
                    let action = menu.items[idx].action;
                    let target_col = menu.col;
                    *column_context_menu = None;
                    if let Some(err) = execute_column_context_action(
                        action,
                        model,
                        target_col,
                        column_list_height,
                    ) {
                        *command_error = Some(err);
                    }
                } else {
                    *column_context_menu = None;
                }
                return;
            }
            MouseEventKind::Down(crossterm::event::MouseButton::Right) => {
                *column_context_menu = None;
            }
            _ => return,
        }
    }

    if let Some(menu) = row_context_menu.as_ref() {
        match mouse.kind {
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                if let Some(idx) = menu.item_at(pos) {
                    let action = menu.items[idx].action;
                    let target_row = menu.row;
                    *row_context_menu = None;
                    if let Some(err) = execute_row_context_action(action, model, target_row) {
                        *command_error = Some(err);
                    }
                } else {
                    *row_context_menu = None;
                }
                return;
            }
            MouseEventKind::Down(crossterm::event::MouseButton::Right) => {
                *row_context_menu = None;
            }
            _ => return,
        }
    }
    let table_width = table_data_width(model, areas.table);
    let viewport_rows = areas.table.height.saturating_sub(3) as usize;

    if let Some(drag) = scrollbar_drag.as_ref() {
        match mouse.kind {
            MouseEventKind::Drag(crossterm::event::MouseButton::Left)
            | MouseEventKind::Moved => {
                apply_scrollbar_drag(model, drag, areas, pos);
            }
            MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
                *scrollbar_drag = None;
            }
            _ => {}
        }
        return;
    }

    if model.view.show_row_json {
        if let Some(popup) = areas.row_json_popup {
            let (viewport_w, viewport_h) = row_json_viewport_size(popup);
            let (content_h, content_w) = model.row_json_content_size();
            let vert_metrics = ScrollMetrics {
                content_length: content_h,
                viewport_length: viewport_h as usize,
                position: model.view.row_json_scroll_y as usize,
            };
            let horiz_metrics = ScrollMetrics {
                content_length: content_w,
                viewport_length: viewport_w as usize,
                position: model.view.row_json_scroll_x as usize,
            };
            if let Some(hit) = vertical_scrollbar_hit(popup, pos, vert_metrics) {
                if matches!(
                    mouse.kind,
                    MouseEventKind::Down(crossterm::event::MouseButton::Left)
                        | MouseEventKind::ScrollUp
                        | MouseEventKind::ScrollDown
                ) {
                    handle_vertical_scroll_hit(
                        mouse.kind,
                        hit,
                        ScrollbarDragTarget::RowJsonVert {
                            viewport_w,
                            viewport_h,
                        },
                        popup,
                        vert_metrics,
                        model,
                        scrollbar_drag,
                    );
                    return;
                }
            }
            if let Some(hit) = horizontal_scrollbar_hit(popup, pos, horiz_metrics) {
                if matches!(
                    mouse.kind,
                    MouseEventKind::Down(crossterm::event::MouseButton::Left)
                        | MouseEventKind::ScrollUp
                        | MouseEventKind::ScrollDown
                ) {
                    handle_horizontal_scroll_hit(
                        mouse.kind,
                        hit,
                        ScrollbarDragTarget::RowJsonHoriz {
                            viewport_w,
                            viewport_h,
                        },
                        popup,
                        horiz_metrics,
                        model,
                        scrollbar_drag,
                    );
                    return;
                }
            }
            match mouse.kind {
                MouseEventKind::ScrollUp if popup.contains(pos) => {
                    model.row_json_scroll_by(0, -3, viewport_w, viewport_h);
                }
                MouseEventKind::ScrollDown if popup.contains(pos) => {
                    model.row_json_scroll_by(0, 3, viewport_w, viewport_h);
                }
                MouseEventKind::Down(crossterm::event::MouseButton::Left) if popup.contains(pos) => {
                    if row_json_resize_hit(popup, pos) {
                        *row_json_resize = Some(FloatingPanelResize {
                            start_mouse_x: col,
                            start_mouse_y: row,
                            start_w: popup.width,
                            start_h: popup.height,
                        });
                    } else if row_json_title_hit(popup, pos) {
                        *row_json_drag = Some(FloatingPanelDrag {
                            grab_x: col.saturating_sub(popup.x),
                            grab_y: row.saturating_sub(popup.y),
                        });
                    }
                }
                _ => {}
            }
        }
        return;
    }

    if model.view.show_column_info {
        if let Some(popup) = areas.column_info_popup {
            let col_idx = model.view.selected_col;
            let viewport = column_info_viewport_lines(popup);
            let total_lines = model.column_info_content_line_count(col_idx);
            let metrics = ScrollMetrics {
                content_length: total_lines,
                viewport_length: viewport as usize,
                position: model.view.column_info_scroll as usize,
            };
            let target = ScrollbarDragTarget::ColumnInfo {
                viewport,
                total_lines,
            };
            if let Some(hit) = vertical_scrollbar_hit(popup, pos, metrics) {
                if matches!(
                    mouse.kind,
                    MouseEventKind::Down(crossterm::event::MouseButton::Left)
                        | MouseEventKind::ScrollUp
                        | MouseEventKind::ScrollDown
                ) {
                    handle_vertical_scroll_hit(
                        mouse.kind,
                        hit,
                        target,
                        popup,
                        metrics,
                        model,
                        scrollbar_drag,
                    );
                    return;
                }
            }
            if matches!(
                mouse.kind,
                MouseEventKind::Down(crossterm::event::MouseButton::Left)
                    | MouseEventKind::ScrollUp
                    | MouseEventKind::ScrollDown
            ) {
                let inner = Block::default().borders(Borders::ALL).inner(popup);
                if inner.contains(pos) {
                    match mouse.kind {
                        MouseEventKind::ScrollUp => model.column_info_scroll_by(-3),
                        MouseEventKind::ScrollDown => model.column_info_scroll_by(3),
                        MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                            let scroll = model.view.column_info_scroll as usize;
                            let line = mouse.row.saturating_sub(inner.y) as usize + scroll;
                            if let Some(option) = column_info_option_at_line(
                                line,
                                model.column_info_type_kinds(col_idx).len(),
                                model.column_info_repr_section_visible(col_idx),
                            ) {
                                model.column_info_apply_option(option);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        return;
    }

    if matches!(
        mouse.kind,
        MouseEventKind::Down(crossterm::event::MouseButton::Left)
            | MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown
    ) && areas.table.contains(pos)
    {
        let row_metrics = table_row_scroll_metrics(model, areas.table);
        let col_metrics = table_col_scroll_metrics(model, areas.table);
        if let Some(hit) = vertical_scrollbar_hit(areas.table, pos, row_metrics) {
            handle_vertical_scroll_hit(
                mouse.kind,
                hit,
                ScrollbarDragTarget::TableRows { viewport_rows },
                areas.table,
                row_metrics,
                model,
                scrollbar_drag,
            );
            return;
        }
        if let Some(hit) = horizontal_scrollbar_hit(areas.table, pos, col_metrics) {
            handle_horizontal_scroll_hit(
                mouse.kind,
                hit,
                ScrollbarDragTarget::TableCols { table_width },
                areas.table,
                col_metrics,
                model,
                scrollbar_drag,
            );
            return;
        }
    }

    if matches!(
        mouse.kind,
        MouseEventKind::Down(crossterm::event::MouseButton::Left)
            | MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown
    ) && areas.columns.contains(pos)
    {
        let metrics = column_list_scroll_metrics(model, areas.columns);
        if let Some(hit) = vertical_scrollbar_hit(areas.columns, pos, metrics) {
            handle_vertical_scroll_hit(
                mouse.kind,
                hit,
                ScrollbarDragTarget::ColumnList {
                    visible_height: column_list_height,
                },
                areas.columns,
                metrics,
                model,
                scrollbar_drag,
            );
            return;
        }
    }

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

    if let Some(resize) = sidebar_resize.as_ref() {
        match mouse.kind {
            MouseEventKind::Drag(crossterm::event::MouseButton::Left)
            | MouseEventKind::Moved => {
                let delta = col as i32 - resize.start_x as i32;
                let max_width = areas
                    .table
                    .width
                    .saturating_add(areas.columns.width)
                    .saturating_sub(MIN_TABLE_WIDTH)
                    .min(MAX_SIDEBAR_WIDTH);
                let new_width = (resize.start_width as i32 - delta)
                    .clamp(MIN_SIDEBAR_WIDTH as i32, max_width as i32) as u16;
                model.view.column_sidebar_width = new_width;
            }
            MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
                *sidebar_resize = None;
            }
            _ => {}
        }
        return;
    }

    if let Some(drag) = cell_range_drag.as_ref() {
        match mouse.kind {
            MouseEventKind::Drag(crossterm::event::MouseButton::Left) => {
                if areas.table.contains(pos) {
                    if let Some(hit) = hit_test_table(col, row, areas.table, model) {
                        if let Some(row_idx) = hit.row {
                            model.set_cell_range_corners(
                                drag.anchor.0,
                                drag.anchor.1,
                                row_idx,
                                hit.col,
                            );
                            model.view.selected_row = row_idx;
                            model.view.selected_col = hit.col;
                        }
                    }
                }
                return;
            }
            MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
                if model.view.cell_range_focus == Some(drag.anchor) {
                    model.clear_cell_range();
                }
                *cell_range_drag = None;
                return;
            }
            _ => {}
        }
    }

    match mouse.kind {
        MouseEventKind::ScrollUp if areas.columns.contains(pos) => {
            model.view.column_sidebar_focused = true;
            model.view.column_list_offset = model
                .view
                .column_list_offset
                .saturating_sub(3);
            model.clamp_column_list_offset(column_list_height);
        }
        MouseEventKind::ScrollDown if areas.columns.contains(pos) => {
            model.view.column_sidebar_focused = true;
            model.view.column_list_offset += 3;
            model.clamp_column_list_offset(column_list_height);
        }
        MouseEventKind::ScrollUp if areas.table.contains(pos) => {
            model.view.column_sidebar_focused = false;
            model.move_selected_row(-3);
        }
        MouseEventKind::ScrollDown if areas.table.contains(pos) => {
            model.view.column_sidebar_focused = false;
            model.move_selected_row(3);
        }
        MouseEventKind::Down(crossterm::event::MouseButton::Left)
            if areas.table.contains(pos) =>
        {
            model.view.column_sidebar_focused = false;
            if let Some(resize_col) = hit_test_column_resize(col, row, areas.table, model) {
                *column_resize = Some(ColumnResize {
                    col: resize_col,
                    start_x: col,
                    start_width: model.column_width_chars(resize_col) as u16,
                });
                return;
            }
            if let Some(row_idx) = hit_test_row_gutter(col, row, areas.table, model) {
                let extend = mouse.modifiers.contains(KeyModifiers::CONTROL);
                if extend {
                    model.add_row_to_selection(row_idx);
                } else {
                    model.select_table_cell_click(
                        row_idx,
                        model.view.selected_col,
                        false,
                        column_list_height,
                    );
                }
                return;
            }
            if let Some(hit) = hit_test_table(col, row, areas.table, model) {
                let extend = mouse.modifiers.contains(KeyModifiers::CONTROL);
                if let Some(row_idx) = hit.row {
                    if extend {
                        model.toggle_cell_multi_select(row_idx, hit.col, column_list_height);
                    } else {
                        model.set_cell_range_corners(row_idx, hit.col, row_idx, hit.col);
                        model.view.selected_row = row_idx;
                        model.view.selected_col = hit.col;
                        model.ensure_column_list_shows_selection(column_list_height);
                        *cell_range_drag = Some(CellRangeDrag {
                            anchor: (row_idx, hit.col),
                        });
                    }
                } else if extend {
                    model.select_table_header_click(hit.col, true, column_list_height);
                } else {
                    model.select_table_header_click(hit.col, false, column_list_height);
                }
            }
        }
        MouseEventKind::Down(crossterm::event::MouseButton::Left)
            if areas.columns.contains(pos) =>
        {
            if hit_test_sidebar_resize(col, row, areas.columns) {
                *sidebar_resize = Some(SidebarResize {
                    start_x: col,
                    start_width: model.view.column_sidebar_width,
                });
                return;
            }
            model.view.column_sidebar_focused = true;
            let inner = Block::default().borders(Borders::ALL).inner(areas.columns);
            if !inner.contains(pos) {
                return;
            }
            let filtered = model.filtered_sidebar_columns();
            let rel = row.saturating_sub(inner.y) as usize;
            let idx = model.view.column_list_offset + rel;
            if let Some(&target_col) = filtered.get(idx) {
                let extend = mouse.modifiers.contains(KeyModifiers::CONTROL);
                model.select_column_click(target_col, extend, column_list_height);
            }
        }
        MouseEventKind::Down(crossterm::event::MouseButton::Right)
            if areas.columns.contains(pos) =>
        {
            if hit_test_sidebar_resize(col, row, areas.columns) {
                return;
            }
            model.view.column_sidebar_focused = true;
            let inner = Block::default().borders(Borders::ALL).inner(areas.columns);
            if !inner.contains(pos) {
                return;
            }
            let filtered = model.filtered_sidebar_columns();
            let rel = row.saturating_sub(inner.y) as usize;
            let idx = model.view.column_list_offset + rel;
            if let Some(&target_col) = filtered.get(idx) {
                let extend = mouse.modifiers.contains(KeyModifiers::CONTROL);
                if extend {
                    model.select_column_click(target_col, true, column_list_height);
                }
                *row_context_menu = None;
                *column_context_menu =
                    Some(ColumnContextMenu::for_column(target_col, model, pos, screen));
            }
        }
        MouseEventKind::Down(crossterm::event::MouseButton::Right)
            if areas.table.contains(pos) =>
        {
            if let Some(row_idx) = hit_test_row_gutter(col, row, areas.table, model) {
                model.view.column_sidebar_focused = false;
                let extend = mouse.modifiers.contains(KeyModifiers::CONTROL);
                if extend {
                    model.set_multi_select_axis(MultiSelectAxis::Row);
                    model.add_row_to_selection(row_idx);
                }
                *column_context_menu = None;
                *row_context_menu =
                    Some(RowContextMenu::for_row(row_idx, model, pos, screen));
                return;
            }
            if let Some(hit) = hit_test_table(col, row, areas.table, model) {
                if hit.row.is_none() {
                    model.view.column_sidebar_focused = false;
                    let extend = mouse.modifiers.contains(KeyModifiers::CONTROL);
                    if extend {
                        model.select_table_header_click(hit.col, true, column_list_height);
                    } else {
                        model.select_table_header_click(hit.col, false, column_list_height);
                    }
                    *row_context_menu = None;
                    *column_context_menu =
                        Some(ColumnContextMenu::for_column(hit.col, model, pos, screen));
                }
            }
        }
        _ => {}
    }
}

struct TableHit {
    row: Option<usize>,
    col: usize,
}

/// X offsets (within the table inner area) of the one-char gap after each visible column
/// except the last. Matches ratatui `Table` layout with `column_spacing(1)`.
fn visible_column_separator_offsets(
    model: &AppModel,
    col_indices: &[usize],
) -> Vec<u16> {
    let mut x = 0u16;
    let mut offsets = Vec::new();
    for (i, &col_idx) in col_indices.iter().enumerate() {
        x = x.saturating_add(model.column_width_chars(col_idx) as u16);
        if i + 1 < col_indices.len() {
            offsets.push(x);
            x = x.saturating_add(1);
        }
    }
    offsets
}

const ROW_GUTTER_WIDTH: u16 = 2;

fn hit_test_row_gutter(
    mouse_x: u16,
    mouse_y: u16,
    table_area: Rect,
    model: &AppModel,
) -> Option<usize> {
    let inner = Block::default().borders(Borders::ALL).inner(table_area);
    if !inner.contains(Position {
        x: mouse_x,
        y: mouse_y,
    }) {
        return None;
    }
    let rel_x = mouse_x.saturating_sub(inner.x);
    if rel_x >= ROW_GUTTER_WIDTH {
        return None;
    }
    let rel_y = mouse_y.saturating_sub(inner.y);
    if rel_y < 2 {
        return None;
    }
    let data_row = (rel_y - 2) as usize;
    let body_height = table_area.height.saturating_sub(3) as usize;
    model.visible_table_rows(body_height).get(data_row).copied()
}

fn table_inner_area(table_area: Rect) -> Rect {
    Block::default().borders(Borders::ALL).inner(table_area)
}

fn table_data_width(model: &AppModel, table_area: Rect) -> u16 {
    let inner = table_inner_area(table_area);
    inner
        .width
        .saturating_sub(ROW_GUTTER_WIDTH.saturating_add(model.column_separator_width() as u16))
}

fn table_data_x_offset(model: &AppModel) -> u16 {
    ROW_GUTTER_WIDTH.saturating_add(model.column_separator_width() as u16)
}

fn row_fully_highlighted(model: &AppModel, row_idx: usize) -> bool {
    model.is_row_multi_selected(row_idx)
}

fn row_indicator_label(model: &AppModel, row_idx: usize) -> &'static str {
    if model.is_row_multi_selected(row_idx) {
        "◆"
    } else if model.view.selected_row == row_idx || model.row_in_cell_range_row_span(row_idx) {
        "▸"
    } else if model.is_row_pinned(row_idx) {
        "▐"
    } else {
        " "
    }
}

fn row_indicator_style(model: &AppModel, row_idx: usize) -> Style {
    if model.is_row_multi_selected(row_idx) {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Blue)
            .add_modifier(Modifier::BOLD)
    } else if model.view.selected_row == row_idx {
        Style::default()
            .fg(Color::Cyan)
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    } else if model.is_row_pinned(row_idx) {
        Style::default().fg(Color::Cyan)
    } else if model.row_in_cell_range_row_span(row_idx) {
        Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

/// Muted column stripe (lower contrast than header/sidebar accents).
fn selected_column_header_style() -> Style {
    Style::default()
        .bg(Color::DarkGray)
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn multi_selected_column_body_style() -> Style {
    Style::default().bg(Color::DarkGray).fg(Color::Blue)
}

fn multi_selected_column_header_style() -> Style {
    Style::default()
        .bg(Color::DarkGray)
        .fg(Color::Blue)
        .add_modifier(Modifier::BOLD)
}

fn column_header_style(model: &AppModel, col_idx: usize) -> Style {
    if col_idx == model.view.selected_col {
        selected_column_header_style()
    } else if model.is_column_multi_selected(col_idx) {
        multi_selected_column_header_style()
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    }
}

fn table_cell_style(model: &AppModel, row_idx: usize, col_idx: usize) -> Style {
    let active_cell =
        row_idx == model.view.selected_row && col_idx == model.view.selected_col;
    if active_cell {
        return Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
    }
    if model.is_cell_in_selection_range(row_idx, col_idx) {
        return Style::default().fg(Color::Black).bg(Color::Blue);
    }
    if model.is_column_multi_selected(col_idx) {
        return multi_selected_column_body_style();
    }
    if row_fully_highlighted(model, row_idx) {
        return Style::default().fg(Color::Black).bg(Color::Blue);
    }
    Style::default()
}

fn column_sidebar_style(model: &AppModel, col_idx: usize) -> Style {
    if col_idx == model.view.selected_col {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Magenta)
            .add_modifier(Modifier::BOLD)
    } else if model.is_column_multi_selected(col_idx) {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Blue)
            .add_modifier(Modifier::BOLD)
    } else if model.is_column_hidden(col_idx) {
        Style::default().fg(Color::DarkGray)
    } else if model.is_column_pinned(col_idx) {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    }
}

fn draw_column_border_lines(
    frame: &mut ratatui::Frame,
    table_area: Rect,
    model: &AppModel,
    col_indices: &[usize],
) {
    if !model.view.show_column_borders {
        return;
    }
    let inner = table_inner_area(table_area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let data_x = inner.x.saturating_add(table_data_x_offset(model));
    let style = Style::default().fg(Color::Gray);
    let sep_xs: Vec<u16> = visible_column_separator_offsets(model, col_indices)
        .into_iter()
        .map(|offset| data_x.saturating_add(offset))
        .collect();
    // Header row + bottom_margin(1) before body rows (matches hit_test_table).
    let header_sep_y = inner.y.saturating_add(1);

    let buf = frame.buffer_mut();
    for y in inner.y..inner.y.saturating_add(inner.height) {
        for &sep_x in &sep_xs {
            if sep_x < inner.x.saturating_add(inner.width) {
                if let Some(cell) = buf.cell_mut((sep_x, y)) {
                    cell.set_char('│').set_style(style);
                }
            }
        }
    }

    if header_sep_y < inner.y.saturating_add(inner.height) {
        for x in inner.x..inner.x.saturating_add(inner.width) {
            let ch = if sep_xs.contains(&x) { '┼' } else { '─' };
            if let Some(cell) = buf.cell_mut((x, header_sep_y)) {
                cell.set_char(ch).set_style(style);
            }
        }
    }
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

    let col_indices = model.visible_table_columns(table_data_width(model, table_area));
    if col_indices.is_empty() {
        return None;
    }

    let rel_x = mouse_x.saturating_sub(inner.x);
    if rel_x < ROW_GUTTER_WIDTH {
        return None;
    }
    let rel_x = rel_x.saturating_sub(table_data_x_offset(model));
    let sep = model.column_separator_width();
    let mut x = 0u16;
    for col_idx in &col_indices {
        let w = model.column_width_chars(*col_idx) as u16;
        let right = x.saturating_add(w);
        if rel_x + 1 >= right && rel_x <= right.saturating_add(sep) {
            return Some(*col_idx);
        }
        x = right.saturating_add(sep);
    }
    None
}

/// Left border of the column sidebar pane (drag to resize width).
fn hit_test_sidebar_resize(mouse_x: u16, mouse_y: u16, columns_area: Rect) -> bool {
    if !columns_area.contains(Position {
        x: mouse_x,
        y: mouse_y,
    }) {
        return false;
    }
    mouse_x <= columns_area.x.saturating_add(1)
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

    let col_indices = model.visible_table_columns(table_data_width(model, table_area));
    if col_indices.is_empty() {
        return None;
    }

    let rel_x = mouse_x.saturating_sub(inner.x);
    let rel_y = mouse_y.saturating_sub(inner.y);
    if rel_x < ROW_GUTTER_WIDTH {
        if rel_y >= 2 {
            let data_row = (rel_y - 2) as usize;
            let body_height = table_area.height.saturating_sub(3) as usize;
            let visible_rows = model.visible_table_rows(body_height);
            if let Some(&row_idx) = visible_rows.get(data_row) {
                return Some(TableHit {
                    row: Some(row_idx),
                    col: model.view.selected_col,
                });
            }
        }
        return None;
    }
    let rel_x = rel_x.saturating_sub(table_data_x_offset(model));
    let sep = model.column_separator_width();
    let mut x = 0u16;
    let mut col_idx = None;
    for &idx in &col_indices {
        let w = model.column_width_chars(idx) as u16;
        let right = x.saturating_add(w);
        if rel_x < right {
            col_idx = Some(idx);
            break;
        }
        x = right.saturating_add(sep);
    }
    let col_idx = col_idx?;
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
    let body_height = table_area.height.saturating_sub(3) as usize;
    let visible_rows = model.visible_table_rows(body_height);
    let Some(&row_idx) = visible_rows.get(data_row) else {
        return None;
    };

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
        row_json_popup: None,
        file_picker_list: Some(layout[1]),
    }
}

fn draw(
    frame: &mut ratatui::Frame,
    model: &mut AppModel,
    command_line: Option<&CommandLineState>,
    command_error: Option<&str>,
    column_finder: Option<&ColumnFinderState>,
    column_context_menu: Option<&ColumnContextMenu>,
    row_context_menu: Option<&RowContextMenu>,
) -> LayoutAreas {
    let area = frame.area();
    let bottom_height = column_finder
        .map(|f| f.panel_height(model))
        .or_else(|| command_line.map(|c| c.panel_height(VIEW_COMMANDS)))
        .unwrap_or(1);

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(bottom_height),
        ])
        .split(area);

    let title = if model.row_value_filters_active() || model.rows_hidden_active() {
        let visible = model.cached_matching_rows().map_or(0, |m| m.len());
        format!(
            " csv  │  {}  │  {visible}/{} rows",
            model.file_label(),
            model.preview.row_count()
        )
    } else {
        format!(
            " csv  │  {}  │  {} rows",
            model.file_label(),
            model.preview.row_count()
        )
    };
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
        .constraints([
            Constraint::Min(MIN_TABLE_WIDTH as u16),
            Constraint::Length(model.view.column_sidebar_width),
        ])
        .split(outer[1]);

    let table_area = main[0];
    let columns_area = main[1];

    draw_table(frame, table_area, model);
    draw_column_list(frame, columns_area, model);

    if let Some(finder) = column_finder {
        finder.draw(frame, outer[2], model);
    } else if let Some(command) = command_line {
        command.draw(frame, outer[2], VIEW_COMMANDS, command_error);
    } else {
        let hints = " q quit  p pin  Space multi-select  r JSON  :hide  :unhide  / columns  :filter  c info  ? help ";
        frame.render_widget(
            Paragraph::new(hints).style(Style::default().fg(Color::DarkGray)),
            outer[2],
        );
    }

    if model.view.show_help {
        draw_help(frame, area);
    }
    if let Some(menu) = column_context_menu {
        draw_column_context_menu(frame, menu);
    }
    if let Some(menu) = row_context_menu {
        draw_row_context_menu(frame, menu);
    }
    let column_info_popup = if model.view.show_column_info {
        let popup_area = centered_rect(54, 72, area);
        draw_column_info(frame, popup_area, model);
        Some(popup_area)
    } else {
        None
    };
    let row_json_popup = if model.view.show_row_json {
        model.clamp_row_json_geometry(area.width, area.height);
        let popup_area = row_json_panel_rect(model, area);
        draw_row_json(frame, popup_area, model);
        Some(popup_area)
    } else {
        None
    };

    LayoutAreas {
        table: table_area,
        columns: columns_area,
        column_info_popup,
        row_json_popup,
        file_picker_list: None,
    }
}

fn draw_table(frame: &mut ratatui::Frame, area: Rect, model: &AppModel) {
    let headers = model.preview.headers();
    if headers.is_empty() {
        frame.render_widget(
            Paragraph::new("No data loaded.")
                .block(Block::default().borders(Borders::ALL))
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    let col_indices = model.visible_table_columns(table_data_width(model, area));
    let visible_headers: Vec<_> = col_indices
        .iter()
        .map(|&i| headers[i].as_str())
        .collect();

    let mut header_cells = vec![Cell::from(" ").style(Style::default())];
    header_cells.extend(
        visible_headers
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let col_idx = col_indices[i];
                Cell::from(model.format_column_header(col_idx, name))
                    .style(column_header_style(model, col_idx))
            }),
    );
    let header = Row::new(header_cells)
        .height(1)
        .bottom_margin(1);

    let body_height = area.height.saturating_sub(3) as usize;
    let visible_row_indices = model.visible_table_rows(body_height);
    let mut rows = Vec::new();
    for &row_idx in &visible_row_indices {
        let Some(fields) = model.preview.row_fields(row_idx) else {
            break;
        };
        let mut cells = vec![Cell::from(row_indicator_label(model, row_idx))
            .style(row_indicator_style(model, row_idx))];
        cells.extend(col_indices.iter().map(|&col_idx| {
            let text = fields.get(col_idx).map(String::as_str).unwrap_or("");
            let display = model.format_column_cell(col_idx, text);
            Cell::from(display).style(table_cell_style(model, row_idx, col_idx))
        }));
        rows.push(Row::new(cells).height(1));
    }

    let mut widths = vec![Constraint::Length(ROW_GUTTER_WIDTH)];
    widths.extend(
        col_indices
            .iter()
            .map(|&col_idx| Constraint::Length(model.column_width_chars(col_idx) as u16)),
    );

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL))
        .column_spacing(1);

    frame.render_widget(table, area);
    draw_column_border_lines(frame, area, model, &col_indices);

    let empty: &[usize] = &[];
    let matching = model.cached_matching_rows().unwrap_or(empty);
    let scrollable_rows = model.scrollable_table_rows(matching);
    let viewport = area.height.saturating_sub(3) as usize;
    let scrollable_visible = model.scrollable_row_visible_count(viewport).max(1);
    render_vertical_scrollbar(
        frame,
        area,
        scrollable_rows.len(),
        scrollable_visible,
        model.view.row_offset,
    );

    let table_cols = model.scrollable_table_columns();
    let scrollable_visible = model
        .scrollable_visible_count(table_data_width(model, area))
        .max(1);
    render_horizontal_scrollbar(
        frame,
        area,
        table_cols.len(),
        scrollable_visible,
        model.view.col_offset,
    );
}

fn draw_column_list(frame: &mut ratatui::Frame, area: Rect, model: &AppModel) {
    let headers = model.preview.headers();
    let filtered = model.filtered_sidebar_columns();
    let inner = Block::default().borders(Borders::ALL).inner(area);
    let visible_height = inner.height.max(1) as usize;
    let mut lines = Vec::new();

    for i in 0..visible_height {
        if let Some(&col_idx) = filtered.get(model.view.column_list_offset + i) {
            let name = headers.get(col_idx).map(String::as_str).unwrap_or("");
            let text = model.format_sidebar_column_label(col_idx, name);
            let mut label = text;
            if model.is_column_pinned(col_idx) {
                label = format!("▐ {label}");
            }
            if model.is_column_hidden(col_idx) {
                label = format!("· {label}");
            }
            let display = truncate_middle(&label, inner.width as usize);
            let style = column_sidebar_style(model, col_idx);
            let prefix = if col_idx == model.view.selected_col {
                "▸ "
            } else if model.is_column_multi_selected(col_idx) {
                "◆ "
            } else {
                "  "
            };
            lines.push(Line::from(Span::styled(format!("{prefix}{display}"), style)));
        } else {
            lines.push(Line::from(" "));
        }
    }

    let end = (model.view.column_list_offset + visible_height).min(filtered.len());
    let title = if model.column_name_filter_active() {
        format!(
            " Columns ({}/{} match \"{}\") ",
            filtered.len(),
            headers.len(),
            model.view.column_name_filter
        )
    } else {
        format!(
            " Columns ({}–{}/{}) ",
            if filtered.is_empty() {
                0
            } else {
                model.view.column_list_offset + 1
            },
            end,
            headers.len()
        )
    };
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta)),
        ),
        area,
    );
    render_vertical_scrollbar(
        frame,
        area,
        filtered.len(),
        visible_height,
        model.view.column_list_offset,
    );
}

fn column_info_viewport_lines(popup_area: Rect) -> u16 {
    Block::default().borders(Borders::ALL).inner(popup_area).height
}

fn column_info_filter_line(type_count: usize, repr_section_visible: bool) -> usize {
    if repr_section_visible {
        3 + type_count + 9
    } else {
        3 + type_count + 2
    }
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
    if line == column_info_filter_line(type_count, repr_section_visible) {
        return Some(if repr_section_visible {
            type_count + 3
        } else {
            type_count
        });
    }
    None
}

fn draw_column_info(frame: &mut ratatui::Frame, popup_area: Rect, model: &mut AppModel) {
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
        " Row filter ",
        Style::default().add_modifier(Modifier::BOLD),
    )));
    let filter_idx = model.column_info_filter_focus_index(col);
    let filter_value = if model.view.column_info_filter_editing {
        model.view.column_info_filter_draft.clone()
    } else {
        model
            .column_value_filter_display(col)
            .unwrap_or("")
            .to_string()
    };
    let filter_marker = if focus == filter_idx { "▸ " } else { "  " };
    let filter_style = if focus == filter_idx || model.view.column_info_filter_editing {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let filter_hint = if model.view.column_info_filter_editing {
        " (Enter apply)"
    } else if model.column_value_filter_is_numeric(col) {
        " e.g. >10, (>=10) & (<20)"
    } else {
        " fuzzy text match"
    };
    let filter_display = if filter_value.is_empty() {
        "(none)".to_string()
    } else {
        filter_value
    };
    lines.push(Line::from(Span::styled(
        format!("{filter_marker}[{filter_display}]{filter_hint}"),
        filter_style,
    )));

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
        " click/↑↓  PgUp/PgDn scroll  Enter  q close ",
        Style::default().fg(Color::DarkGray),
    )));

    let block = Block::default()
        .title(" Column info ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta));
    let viewport = column_info_viewport_lines(popup_area);
    model.column_info_clamp_scroll(viewport, lines.len());
    let scroll = model.view.column_info_scroll;
    let line_count = lines.len();

    frame.render_widget(
        Paragraph::new(lines).block(block).scroll((scroll, 0)),
        popup_area,
    );
    render_vertical_scrollbar(frame, popup_area, line_count, viewport as usize, scroll as usize);
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

fn row_json_panel_rect(model: &AppModel, screen: Rect) -> Rect {
    let default = centered_rect(60, 50, screen);
    let width = model
        .view
        .row_json_width
        .unwrap_or(default.width)
        .clamp(AppModel::MIN_ROW_JSON_WIDTH, screen.width.max(AppModel::MIN_ROW_JSON_WIDTH));
    let height = model
        .view
        .row_json_height
        .unwrap_or(default.height)
        .clamp(AppModel::MIN_ROW_JSON_HEIGHT, screen.height.max(AppModel::MIN_ROW_JSON_HEIGHT));
    let max_x = screen.width.saturating_sub(width);
    let max_y = screen.height.saturating_sub(height);
    let x = model.view.row_json_x.unwrap_or(default.x).min(max_x);
    let y = model.view.row_json_y.unwrap_or(default.y).min(max_y);
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn row_json_viewport_size(area: Rect) -> (u16, u16) {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    (
        inner.width.saturating_sub(1).max(1),
        inner.height.saturating_sub(1).max(1),
    )
}

fn row_json_title_hit(area: Rect, pos: Position) -> bool {
    pos.y == area.y && pos.x >= area.x && pos.x < area.x.saturating_add(area.width)
}

fn row_json_resize_hit(area: Rect, pos: Position) -> bool {
    let right = area.x.saturating_add(area.width.saturating_sub(1));
    let bottom = area.y.saturating_add(area.height.saturating_sub(1));
    pos.x >= right.saturating_sub(1) && pos.y >= bottom.saturating_sub(1) && area.contains(pos)
}

fn draw_row_json(frame: &mut ratatui::Frame, popup_area: Rect, model: &mut AppModel) {
    frame.render_widget(Clear, popup_area);
    let lines = model.row_json_lines();
    let (content_h, content_w) = model.row_json_content_size();
    let (viewport_w, viewport_h) = row_json_viewport_size(popup_area);
    model.clamp_row_json_scroll(viewport_w, viewport_h);
    let scroll_x = model.view.row_json_scroll_x;
    let scroll_y = model.view.row_json_scroll_y;
    let row_idx = model.view.selected_row;
    let title = format!(" Row {} JSON  drag title · resize corner · q close ", row_idx + 1);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let text_lines: Vec<Line> = lines.into_iter().map(Line::from).collect();
    frame.render_widget(
        Paragraph::new(text_lines)
            .block(block)
            .scroll((scroll_y, scroll_x)),
        popup_area,
    );
    render_vertical_scrollbar(
        frame,
        popup_area,
        content_h,
        viewport_h as usize,
        scroll_y as usize,
    );
    render_horizontal_scrollbar(
        frame,
        popup_area,
        content_w,
        viewport_w as usize,
        scroll_x as usize,
    );
}

fn render_vertical_scrollbar(
    frame: &mut ratatui::Frame,
    area: Rect,
    content_length: usize,
    viewport_length: usize,
    position: usize,
) {
    if content_length <= viewport_length || viewport_length == 0 {
        return;
    }
    let metrics = ScrollMetrics {
        content_length,
        viewport_length,
        position,
    };
    // Ratatui treats content_length-1 as the last thumb position; pass
    // max_scroll+1 so the thumb sits flush at the track end when scrolled fully.
    let mut state = ScrollbarState::new(metrics.scrollbar_state_content_length())
        .viewport_content_length(viewport_length)
        .position(position.min(metrics.max_position()));
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼")),
        area.inner(Margin {
            vertical: 1,
            horizontal: 0,
        }),
        &mut state,
    );
}

fn render_horizontal_scrollbar(
    frame: &mut ratatui::Frame,
    area: Rect,
    content_length: usize,
    viewport_length: usize,
    position: usize,
) {
    if content_length <= viewport_length || viewport_length == 0 {
        return;
    }
    let metrics = ScrollMetrics {
        content_length,
        viewport_length,
        position,
    };
    let mut state = ScrollbarState::new(metrics.scrollbar_state_content_length())
        .viewport_content_length(viewport_length)
        .position(position.min(metrics.max_position()));
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::HorizontalBottom)
            .begin_symbol(Some("◀"))
            .end_symbol(Some("▶")),
        area.inner(Margin {
            vertical: 0,
            horizontal: 1,
        }),
        &mut state,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{MouseButton, MouseEvent};

    fn test_model() -> Option<AppModel> {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return None;
        }
        Some(AppModel::open(Some(path)).expect("open csv"))
    }

    fn empty_mouse_state() -> (
        Option<ColumnResize>,
        Option<SidebarResize>,
        Option<CellRangeDrag>,
        Option<ScrollbarDrag>,
        Option<FloatingPanelDrag>,
        Option<FloatingPanelResize>,
        Option<ColumnContextMenu>,
        Option<RowContextMenu>,
        Option<String>,
    ) {
        (None, None, None, None, None, None, None, None, None)
    }

    #[test]
    fn ctrl_click_table_header_multi_selects_columns() {
        let Some(mut model) = test_model() else {
            return;
        };
        // Default column widths (MIN_COLUMN_WIDTH) give a deterministic layout.
        let table_area = Rect::new(0, 3, 40, 20);
        let areas = LayoutAreas {
            table: table_area,
            columns: Rect::default(),
            column_info_popup: None,
            row_json_popup: None,
            file_picker_list: None,
        };
        let screen = Rect::new(0, 0, 80, 30);
        let inner = table_inner_area(table_area);
        let col_indices = model.visible_table_columns(table_data_width(&model, table_area));
        assert!(col_indices.len() >= 2, "need at least 2 visible columns for this test");

        let header_row = inner.y;
        let col0_mid_x = inner.x + table_data_x_offset(&model) + 2;
        let col1_mid_x = inner.x + table_data_x_offset(&model) + 7;

        let (
            mut column_resize,
            mut sidebar_resize,
            mut cell_range_drag,
            mut scrollbar_drag,
            mut row_json_drag,
            mut row_json_resize,
            mut column_context_menu,
            mut row_context_menu,
            mut command_error,
        ) = empty_mouse_state();

        handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: col0_mid_x,
                row: header_row,
                modifiers: KeyModifiers::NONE,
            },
            &mut model,
            &areas,
            20,
            &mut column_resize,
            &mut sidebar_resize,
            &mut cell_range_drag,
            &mut scrollbar_drag,
            &mut row_json_drag,
            &mut row_json_resize,
            &mut column_context_menu,
            &mut row_context_menu,
            &mut command_error,
            screen,
        );
        assert_eq!(model.view.selected_col, col_indices[0]);
        assert!(model.view.multi_selected_cols.is_empty());

        handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: col1_mid_x,
                row: header_row,
                modifiers: KeyModifiers::CONTROL,
            },
            &mut model,
            &areas,
            20,
            &mut column_resize,
            &mut sidebar_resize,
            &mut cell_range_drag,
            &mut scrollbar_drag,
            &mut row_json_drag,
            &mut row_json_resize,
            &mut column_context_menu,
            &mut row_context_menu,
            &mut command_error,
            screen,
        );
        assert_eq!(model.view.selected_col, col_indices[1]);
        assert_eq!(
            model.view.multi_selected_cols,
            vec![col_indices[0], col_indices[1]]
        );
        assert_eq!(command_error, None);
    }

    #[test]
    fn context_menu_select_multi_selects_columns_without_ctrl() {
        let Some(mut model) = test_model() else {
            return;
        };
        let table_area = Rect::new(0, 3, 50, 20);
        let columns_area = Rect::new(50, 3, 30, 20);
        let areas = LayoutAreas {
            table: table_area,
            columns: columns_area,
            column_info_popup: None,
            row_json_popup: None,
            file_picker_list: None,
        };
        let screen = Rect::new(0, 0, 80, 30);
        let table_inner = table_inner_area(table_area);
        let sidebar_inner = Block::default().borders(Borders::ALL).inner(columns_area);
        let col_indices = model.visible_table_columns(table_data_width(&model, table_area));
        assert!(col_indices.len() >= 3, "need at least 3 visible columns for this test");

        let header_row = table_inner.y;
        let col0_mid_x = table_inner.x + table_data_x_offset(&model) + 2;
        let sidebar_col1_x = sidebar_inner.x + 2;
        let sidebar_col2_x = sidebar_inner.x + 2;
        let sidebar_col1_y = sidebar_inner.y + 1;
        let sidebar_col2_y = sidebar_inner.y + 2;

        let (
            mut column_resize,
            mut sidebar_resize,
            mut cell_range_drag,
            mut scrollbar_drag,
            mut row_json_drag,
            mut row_json_resize,
            mut column_context_menu,
            mut row_context_menu,
            mut command_error,
        ) = empty_mouse_state();

        handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: col0_mid_x,
                row: header_row,
                modifiers: KeyModifiers::NONE,
            },
            &mut model,
            &areas,
            20,
            &mut column_resize,
            &mut sidebar_resize,
            &mut cell_range_drag,
            &mut scrollbar_drag,
            &mut row_json_drag,
            &mut row_json_resize,
            &mut column_context_menu,
            &mut row_context_menu,
            &mut command_error,
            screen,
        );
        assert_eq!(model.view.selected_col, col_indices[0]);

        handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Right),
                column: sidebar_col1_x,
                row: sidebar_col1_y,
                modifiers: KeyModifiers::NONE,
            },
            &mut model,
            &areas,
            20,
            &mut column_resize,
            &mut sidebar_resize,
            &mut cell_range_drag,
            &mut scrollbar_drag,
            &mut row_json_drag,
            &mut row_json_resize,
            &mut column_context_menu,
            &mut row_context_menu,
            &mut command_error,
            screen,
        );
        let menu = column_context_menu.as_ref().expect("menu opened on column 1");
        assert_eq!(menu.col, col_indices[1]);
        let idx = menu
            .items
            .iter()
            .position(|item| matches!(item.action, ColumnContextAction::Select))
            .expect("Select item present");
        assert_eq!(menu.items[idx].label, "Select");
        let click_pos = Position {
            x: menu.area.x + 1,
            y: menu.area.y + 1 + idx as u16,
        };

        handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: click_pos.x,
                row: click_pos.y,
                modifiers: KeyModifiers::NONE,
            },
            &mut model,
            &areas,
            20,
            &mut column_resize,
            &mut sidebar_resize,
            &mut cell_range_drag,
            &mut scrollbar_drag,
            &mut row_json_drag,
            &mut row_json_resize,
            &mut column_context_menu,
            &mut row_context_menu,
            &mut command_error,
            screen,
        );
        assert!(column_context_menu.is_none(), "menu should close after activation");
        assert_eq!(
            model.view.multi_selected_cols,
            vec![col_indices[0], col_indices[1]]
        );
        assert_eq!(model.view.selected_col, col_indices[1]);

        handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Right),
                column: sidebar_col2_x,
                row: sidebar_col2_y,
                modifiers: KeyModifiers::NONE,
            },
            &mut model,
            &areas,
            20,
            &mut column_resize,
            &mut sidebar_resize,
            &mut cell_range_drag,
            &mut scrollbar_drag,
            &mut row_json_drag,
            &mut row_json_resize,
            &mut column_context_menu,
            &mut row_context_menu,
            &mut command_error,
            screen,
        );
        let menu = column_context_menu.as_ref().expect("menu opened on column 2");
        let idx = menu
            .items
            .iter()
            .position(|item| matches!(item.action, ColumnContextAction::Select))
            .expect("Select item present");
        let click_pos = Position {
            x: menu.area.x + 1,
            y: menu.area.y + 1 + idx as u16,
        };
        handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: click_pos.x,
                row: click_pos.y,
                modifiers: KeyModifiers::NONE,
            },
            &mut model,
            &areas,
            20,
            &mut column_resize,
            &mut sidebar_resize,
            &mut cell_range_drag,
            &mut scrollbar_drag,
            &mut row_json_drag,
            &mut row_json_resize,
            &mut column_context_menu,
            &mut row_context_menu,
            &mut command_error,
            screen,
        );
        assert_eq!(
            model.view.multi_selected_cols,
            vec![col_indices[0], col_indices[1], col_indices[2]]
        );
        assert_eq!(command_error, None);
    }

    #[test]
    fn ctrl_click_table_body_cell_toggles_individual_cell() {
        let Some(mut model) = test_model() else {
            return;
        };
        let table_area = Rect::new(0, 3, 40, 20);
        let areas = LayoutAreas {
            table: table_area,
            columns: Rect::default(),
            column_info_popup: None,
            row_json_popup: None,
            file_picker_list: None,
        };
        let screen = Rect::new(0, 0, 80, 30);
        let inner = table_inner_area(table_area);
        let col0_mid_x = inner.x + table_data_x_offset(&model) + 2;
        let data_row = inner.y + 2;

        let (
            mut column_resize,
            mut sidebar_resize,
            mut cell_range_drag,
            mut scrollbar_drag,
            mut row_json_drag,
            mut row_json_resize,
            mut column_context_menu,
            mut row_context_menu,
            mut command_error,
        ) = empty_mouse_state();

        handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: col0_mid_x,
                row: data_row,
                modifiers: KeyModifiers::CONTROL,
            },
            &mut model,
            &areas,
            20,
            &mut column_resize,
            &mut sidebar_resize,
            &mut cell_range_drag,
            &mut scrollbar_drag,
            &mut row_json_drag,
            &mut row_json_resize,
            &mut column_context_menu,
            &mut row_context_menu,
            &mut command_error,
            screen,
        );
        assert!(model.view.multi_selected_cols.is_empty());
        assert!(!cell_range_drag.is_some());
        let visible_rows = model.visible_table_rows(table_area.height.saturating_sub(3) as usize);
        let row_idx = visible_rows.first().copied().expect("visible row");
        assert!(model.is_cell_multi_selected(row_idx, 0));
    }

    #[test]
    fn plain_drag_table_body_starts_cell_range() {
        let Some(mut model) = test_model() else {
            return;
        };
        let table_area = Rect::new(0, 3, 40, 20);
        let areas = LayoutAreas {
            table: table_area,
            columns: Rect::default(),
            column_info_popup: None,
            row_json_popup: None,
            file_picker_list: None,
        };
        let screen = Rect::new(0, 0, 80, 30);
        let inner = table_inner_area(table_area);
        let col0_mid_x = inner.x + table_data_x_offset(&model) + 2;
        let data_row = inner.y + 2;

        let (
            mut column_resize,
            mut sidebar_resize,
            mut cell_range_drag,
            mut scrollbar_drag,
            mut row_json_drag,
            mut row_json_resize,
            mut column_context_menu,
            mut row_context_menu,
            mut command_error,
        ) = empty_mouse_state();

        handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: col0_mid_x,
                row: data_row,
                modifiers: KeyModifiers::NONE,
            },
            &mut model,
            &areas,
            20,
            &mut column_resize,
            &mut sidebar_resize,
            &mut cell_range_drag,
            &mut scrollbar_drag,
            &mut row_json_drag,
            &mut row_json_resize,
            &mut column_context_menu,
            &mut row_context_menu,
            &mut command_error,
            screen,
        );
        assert!(cell_range_drag.is_some());
        assert!(model.cell_range_active());
    }
}
