use crate::column::{infer_column_kind, is_right_aligned, ColumnKind};
use crate::schema;
use std::path::PathBuf;

pub const CELL_DISPLAY_WIDTH: usize = 18;

#[derive(Debug, Clone)]
pub struct TableViewState {
    pub selected_row: usize,
    pub selected_col: usize,
    pub row_offset: usize,
    pub col_offset: usize,
    /// Independent scroll position for the column sidebar (not tied to selection).
    pub column_list_offset: usize,
    pub show_column_types: bool,
    pub show_help: bool,
}

impl Default for TableViewState {
    fn default() -> Self {
        Self {
            selected_row: 0,
            selected_col: 0,
            row_offset: 0,
            col_offset: 0,
            column_list_offset: 0,
            show_column_types: false,
            show_help: false,
        }
    }
}

/// Serializable snapshot for HTTP/WebSocket clients.
#[derive(Debug, Clone)]
pub struct ViewSnapshot {
    pub file_path: Option<PathBuf>,
    pub headers: Vec<String>,
    pub row_count: usize,
    pub scan_done: bool,
    pub scan_error: bool,
    pub selected_row: usize,
    pub selected_col: usize,
    pub visible_rows: Vec<Vec<String>>,
    pub visible_row_indices: Vec<usize>,
    pub visible_columns: Vec<VisibleColumn>,
    pub sidebar_columns: Vec<SidebarColumn>,
    pub status_line: String,
}

#[derive(Debug, Clone)]
pub struct VisibleColumn {
    pub index: usize,
    pub name: String,
    pub width: u16,
    pub kind: ColumnKind,
    pub align_right: bool,
}

#[derive(Debug, Clone)]
pub struct SidebarColumn {
    pub index: usize,
    pub label: String,
    pub selected: bool,
}

#[derive(Debug)]
pub struct AppModel {
    pub file_path: Option<PathBuf>,
    pub preview: crate::preview::PreviewData,
    pub view: TableViewState,
    pub scan_thread: Option<std::thread::JoinHandle<()>>,
}

impl AppModel {
    pub fn open(file_path: Option<PathBuf>) -> std::io::Result<Self> {
        let preview = match &file_path {
            Some(path) => crate::preview::PreviewData::load_header_and_initial_rows(
                path,
                crate::preview::INITIAL_BODY_LINES,
            )?,
            None => crate::preview::PreviewData::empty(),
        };

        let scan_thread = file_path.as_ref().map(|path| {
            let skip = preview.row_count();
            preview.start_background_scan(path, skip)
        });

        Ok(Self {
            file_path,
            preview,
            view: TableViewState::default(),
            scan_thread,
        })
    }

    pub fn join_scan_thread(&mut self) {
        if let Some(handle) = self.scan_thread.take() {
            let _ = handle.join();
        }
    }

    pub fn file_label(&self) -> &str {
        self.file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("<no file>")
    }

    pub fn max_visible_columns(&self, table_width: u16) -> usize {
        let col_slot = (CELL_DISPLAY_WIDTH + 1) as u16;
        (table_width / col_slot.max(1)).max(1) as usize
    }

    pub fn clamp_selection(&mut self, viewport_rows: usize, table_width: u16) {
        let max_rows = self.preview.row_count().saturating_sub(1);
        let headers = self.preview.headers();
        let max_cols = headers.len().saturating_sub(1);
        let max_visible = self.max_visible_columns(table_width);

        self.view.selected_row = self.view.selected_row.min(max_rows);
        self.view.selected_col = self.view.selected_col.min(max_cols);

        if self.view.selected_row < self.view.row_offset {
            self.view.row_offset = self.view.selected_row;
        }
        if viewport_rows > 0 && self.view.selected_row >= self.view.row_offset + viewport_rows {
            self.view.row_offset = self.view.selected_row - viewport_rows + 1;
        }

        if self.view.selected_col < self.view.col_offset {
            self.view.col_offset = self.view.selected_col;
        }
        if max_visible > 0 && self.view.selected_col >= self.view.col_offset + max_visible {
            self.view.col_offset = self.view.selected_col - max_visible + 1;
        }
    }

    pub fn clamp_column_list_offset(&mut self, visible_height: usize) {
        let header_count = self.preview.headers().len();
        let max_offset = header_count.saturating_sub(visible_height.max(1));
        self.view.column_list_offset = self.view.column_list_offset.min(max_offset);
    }

    pub fn ensure_column_list_shows_selection(&mut self, visible_height: usize) {
        let sel = self.view.selected_col;
        let off = self.view.column_list_offset;
        if sel < off {
            self.view.column_list_offset = sel;
        } else if visible_height > 0 && sel >= off + visible_height {
            self.view.column_list_offset = sel - visible_height + 1;
        }
        self.clamp_column_list_offset(visible_height);
    }

    pub fn visible_column_range(&self, table_width: u16) -> std::ops::Range<usize> {
        let headers_len = self.preview.headers().len();
        let max_visible = self.max_visible_columns(table_width);
        let start = self.view.col_offset;
        let end = (start + max_visible).min(headers_len);
        start..end
    }

    pub fn snapshot(&self, viewport_rows: usize, table_width: u16) -> ViewSnapshot {
        let headers = self.preview.headers();
        let col_range = self.visible_column_range(table_width);

        let mut visible_rows = Vec::new();
        let mut visible_row_indices = Vec::new();
        for i in 0..viewport_rows {
            let row_idx = self.view.row_offset + i;
            if let Some(line) = self.preview.row_line(row_idx) {
                let fields = schema::split_row(&line);
                visible_row_indices.push(row_idx);
                visible_rows.push(fields);
            }
        }

        let visible_columns = col_range
            .clone()
            .filter_map(|col_idx| {
                let name = headers.get(col_idx)?.clone();
                let kind = infer_column_kind(&name);
                Some(VisibleColumn {
                    index: col_idx,
                    name,
                    width: CELL_DISPLAY_WIDTH as u16,
                    kind,
                    align_right: is_right_aligned(kind),
                })
            })
            .collect();

        let sidebar_columns = headers
            .iter()
            .enumerate()
            .map(|(col_idx, name)| {
                let kind = infer_column_kind(name);
                let label = if self.view.show_column_types {
                    format!("{col_idx}: {name} [{}]", kind.label())
                } else {
                    format!("{col_idx}: {name}")
                };
                SidebarColumn {
                    index: col_idx,
                    label,
                    selected: col_idx == self.view.selected_col,
                }
            })
            .collect();

        let status_line = format!(
            "row {}/{}  col {}/{}  {}",
            self.view.selected_row + 1,
            self.preview.row_count().max(1),
            self.view.selected_col + 1,
            headers.len().max(1),
            if self.preview.scan_done() {
                "loaded"
            } else {
                "loading…"
            }
        );

        ViewSnapshot {
            file_path: self.file_path.clone(),
            headers,
            row_count: self.preview.row_count(),
            scan_done: self.preview.scan_done(),
            scan_error: self.preview.scan_error(),
            selected_row: self.view.selected_row,
            selected_col: self.view.selected_col,
            visible_rows,
            visible_row_indices,
            visible_columns,
            sidebar_columns,
            status_line,
        }
    }
}

/// Format a cell for fixed-width display (ASCII-only, `~` truncation marker).
pub fn format_cell(text: &str, width: usize, align_right: bool) -> String {
    if width == 0 {
        return String::new();
    }
    let mut buf = vec![b' '; width];
    let truncated = text.len() > width;
    let take = text.len().min(width);
    let visible: String = text
        .bytes()
        .take(take)
        .map(|b| if (32..=126).contains(&b) { b as char } else { '.' })
        .collect();
    let vis_len = if truncated { width } else { visible.len() };
    if align_right {
        let start = width.saturating_sub(vis_len);
        for (i, ch) in visible.chars().take(vis_len).enumerate() {
            if start + i < width {
                buf[start + i] = ch as u8;
            }
        }
    } else {
        for (i, ch) in visible.chars().take(vis_len).enumerate() {
            buf[i] = ch as u8;
        }
    }
    if truncated {
        buf[width - 1] = b'~';
    }
    String::from_utf8(buf).unwrap_or_else(|_| " ".repeat(width))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn truncates_long_cells() {
        assert_eq!(format_cell("hello world", 5, false), "hell~");
    }

    #[test]
    fn column_list_scrolls_independently_of_row_count() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        assert_eq!(model.preview.headers().len(), 100);
        assert!(model.preview.row_count() < 100);

        model.view.column_list_offset = 90;
        model.clamp_column_list_offset(10);
        assert_eq!(model.view.column_list_offset, 90);
    }
}
