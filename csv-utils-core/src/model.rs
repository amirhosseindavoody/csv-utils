use crate::column::{
    column_kind_index, column_kind_options, infer_kind_from_state, is_right_aligned,
    is_numeric, observe_column_infer, ColumnInferState,
    ColumnKind, NumericRepr,
};
use crate::column_stats::{build_column_info, ColumnInfo, ColumnStatsAccum};
use crate::display::{format_cell_for_column, sanitize_ascii, truncate_middle};
use crate::schema;
use std::path::PathBuf;

pub const MIN_COLUMN_WIDTH: usize = 4;
pub const MAX_COLUMN_WIDTH: usize = 64;

#[derive(Debug, Clone)]
pub struct TableViewState {
    pub selected_row: usize,
    pub selected_col: usize,
    pub row_offset: usize,
    pub col_offset: usize,
    /// Independent scroll position for the column sidebar (not tied to selection).
    pub column_list_offset: usize,
    /// Display width in characters, indexed by column.
    pub column_widths: Vec<u16>,
    /// Per-column type override (`Auto` = infer from loaded rows).
    pub column_kinds: Vec<ColumnKind>,
    /// General vs scientific formatting for numeric columns.
    pub column_numeric_repr: Vec<NumericRepr>,
    /// Manual resize lock per column (until file reopen).
    pub column_widths_user_set: Vec<bool>,
    pub show_column_format: bool,
    pub show_column_info: bool,
    /// Focus index in the format pane (0–4 type, 5–6 representation).
    pub column_format_focus: usize,
    pub show_help: bool,
}

#[derive(Debug, Default)]
struct ColumnLayoutCache {
    rows_processed: usize,
    max_content_len: Vec<usize>,
    infer_state: Vec<ColumnInferState>,
    stats: Vec<ColumnStatsAccum>,
}

impl Default for TableViewState {
    fn default() -> Self {
        Self {
            selected_row: 0,
            selected_col: 0,
            row_offset: 0,
            col_offset: 0,
            column_list_offset: 0,
            column_widths: Vec::new(),
            column_kinds: Vec::new(),
            column_numeric_repr: Vec::new(),
            column_widths_user_set: Vec::new(),
            show_column_format: false,
            show_column_info: false,
            column_format_focus: 0,
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
    column_layout: ColumnLayoutCache,
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

        let mut model = Self {
            file_path,
            preview,
            view: TableViewState::default(),
            scan_thread,
            column_layout: ColumnLayoutCache::default(),
        };
        model.ensure_column_state();
        model.maybe_update_column_layout();
        Ok(model)
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

    pub fn ensure_column_state(&mut self) {
        let n = self.preview.headers().len();
        if self.view.column_widths.len() != n {
            self.view.column_widths = vec![MIN_COLUMN_WIDTH as u16; n];
        }
        if self.view.column_kinds.len() != n {
            self.view.column_kinds = vec![ColumnKind::Auto; n];
        }
        if self.view.column_numeric_repr.len() != n {
            self.view.column_numeric_repr = vec![NumericRepr::General; n];
        }
        if self.view.column_widths_user_set.len() != n {
            self.view.column_widths_user_set = vec![false; n];
        }
    }

    pub fn column_width_chars(&self, col: usize) -> usize {
        self.view
            .column_widths
            .get(col)
            .copied()
            .unwrap_or(MIN_COLUMN_WIDTH as u16) as usize
    }

    fn reset_column_layout_cache(&mut self, headers: &[String]) {
        let n = headers.len();
        self.column_layout.rows_processed = 0;
        self.column_layout.max_content_len = headers
            .iter()
            .map(|h| sanitize_ascii(h).len())
            .collect();
        self.column_layout.infer_state = vec![ColumnInferState::Unknown; n];
        self.column_layout.stats = vec![ColumnStatsAccum::default(); n];
    }

    fn apply_fitted_column_widths(&mut self) {
        for col in 0..self.column_layout.max_content_len.len() {
            if self
                .view
                .column_widths_user_set
                .get(col)
                .copied()
                .unwrap_or(false)
            {
                continue;
            }
            let w = self.column_layout.max_content_len[col].clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH);
            self.view.column_widths[col] = w as u16;
        }
    }

    fn apply_fitted_column_width(&mut self, col: usize) {
        if self
            .view
            .column_widths_user_set
            .get(col)
            .copied()
            .unwrap_or(false)
        {
            return;
        }
        if let Some(&max_len) = self.column_layout.max_content_len.get(col) {
            let w = max_len.clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH);
            self.view.column_widths[col] = w as u16;
        }
    }

    /// Incrementally scan newly loaded rows for width auto-fit and type inference.
    pub fn maybe_update_column_layout(&mut self) {
        self.ensure_column_state();
        let headers = self.preview.headers();
        let n = headers.len();
        if self.column_layout.max_content_len.len() != n {
            self.reset_column_layout_cache(&headers);
        }

        let row_count = self.preview.row_count();
        let start = self.column_layout.rows_processed;
        if start >= row_count {
            return;
        }

        for row_idx in start..row_count {
            let Some(line) = self.preview.row_line(row_idx) else {
                break;
            };
            let fields = schema::split_row(&line);
            for col in 0..n {
                if let Some(cell) = fields.get(col) {
                    let len = sanitize_ascii(cell).len();
                    self.column_layout.max_content_len[col] =
                        self.column_layout.max_content_len[col].max(len);
                    observe_column_infer(&mut self.column_layout.infer_state[col], cell);
                    self.column_layout.stats[col].observe(cell);
                }
            }
        }
        self.column_layout.rows_processed = row_count;
        self.apply_fitted_column_widths();
    }

    pub fn effective_column_kind(&self, col: usize) -> ColumnKind {
        let stored = self
            .view
            .column_kinds
            .get(col)
            .copied()
            .unwrap_or(ColumnKind::Auto);
        if stored != ColumnKind::Auto {
            return stored;
        }
        infer_kind_from_state(
            self.column_layout
                .infer_state
                .get(col)
                .copied()
                .unwrap_or(ColumnInferState::Unknown),
        )
    }

    pub fn numeric_repr(&self, col: usize) -> NumericRepr {
        self.view
            .column_numeric_repr
            .get(col)
            .copied()
            .unwrap_or(NumericRepr::General)
    }

    pub fn format_column_cell(&self, col: usize, text: &str) -> String {
        let width = self.column_width_chars(col);
        let kind = self.effective_column_kind(col);
        let repr = self.numeric_repr(col);
        format_cell_for_column(text, width, kind, repr)
    }

    pub fn format_column_header(&self, col: usize, name: &str) -> String {
        let width = self.column_width_chars(col);
        let visible = sanitize_ascii(name);
        if visible.len() <= width {
            format!("{visible:<width$}", width = width)
        } else {
            format!("{:<width$}", truncate_middle(name, width), width = width)
        }
    }

    pub fn refit_column_widths(&mut self) {
        self.apply_fitted_column_widths();
    }

    pub fn maybe_refit_column_widths(&mut self) {
        self.maybe_update_column_layout();
    }

    pub fn set_column_width(&mut self, col: usize, width: u16) {
        self.ensure_column_state();
        if col < self.view.column_widths.len() {
            self.view.column_widths[col] =
                width.clamp(MIN_COLUMN_WIDTH as u16, MAX_COLUMN_WIDTH as u16);
            self.view.column_widths_user_set[col] = true;
        }
    }

    pub fn set_column_kind(&mut self, col: usize, kind: ColumnKind) {
        self.ensure_column_state();
        if col < self.view.column_kinds.len() {
            self.view.column_kinds[col] = kind;
            self.view.column_widths_user_set[col] = false;
            self.apply_fitted_column_width(col);
        }
    }

    pub fn set_numeric_repr(&mut self, col: usize, repr: NumericRepr) {
        self.ensure_column_state();
        if col < self.view.column_numeric_repr.len() {
            self.view.column_numeric_repr[col] = repr;
            self.view.column_widths_user_set[col] = false;
            self.apply_fitted_column_width(col);
        }
    }

    pub fn open_column_format_pane(&mut self) {
        let col = self.view.selected_col;
        self.view.show_column_format = true;
        let stored = self
            .view
            .column_kinds
            .get(col)
            .copied()
            .unwrap_or(ColumnKind::Auto);
        self.view.column_format_focus = column_kind_index(stored);
    }

    pub fn close_column_format_pane(&mut self) {
        self.view.show_column_format = false;
    }

    pub fn column_format_repr_enabled(&self) -> bool {
        let col = self.view.selected_col;
        let focus = self.view.column_format_focus;
        if (3..=4).contains(&focus) {
            return true;
        }
        let stored = self
            .view
            .column_kinds
            .get(col)
            .copied()
            .unwrap_or(ColumnKind::Auto);
        if matches!(stored, ColumnKind::Int | ColumnKind::Float) {
            return true;
        }
        if stored == ColumnKind::Auto {
            return is_numeric(self.effective_column_kind(col));
        }
        false
    }

    pub fn column_format_focus_max(&self) -> usize {
        if self.column_format_repr_enabled() {
            6
        } else {
            4
        }
    }

    pub fn column_format_focus_delta(&mut self, delta: i32) {
        let max = self.column_format_focus_max() as i32;
        let next = self.view.column_format_focus as i32 + delta;
        self.view.column_format_focus = next.clamp(0, max) as usize;
    }

    pub fn column_format_apply_focus(&mut self) {
        let col = self.view.selected_col;
        match self.view.column_format_focus {
            0..=4 => {
                let kind = column_kind_options()[self.view.column_format_focus];
                self.set_column_kind(col, kind);
            }
            5 => self.set_numeric_repr(col, NumericRepr::General),
            6 => self.set_numeric_repr(col, NumericRepr::Scientific),
            _ => {}
        }
    }

    pub fn stored_column_kind(&self, col: usize) -> ColumnKind {
        self.view
            .column_kinds
            .get(col)
            .copied()
            .unwrap_or(ColumnKind::Auto)
    }

    pub fn open_column_info_pane(&mut self) {
        self.view.show_column_info = true;
    }

    pub fn close_column_info_pane(&mut self) {
        self.view.show_column_info = false;
    }

    pub fn column_info(&self, col: usize) -> ColumnInfo {
        let headers = self.preview.headers();
        let name = headers.get(col).map(String::as_str).unwrap_or("");
        let stored = self.stored_column_kind(col);
        let effective = self.effective_column_kind(col);
        let repr = self.numeric_repr(col);
        let stats = self
            .column_layout
            .stats
            .get(col)
            .cloned()
            .unwrap_or_default();
        build_column_info(
            col,
            name,
            stored,
            effective,
            repr,
            &stats,
            self.preview.scan_done(),
        )
    }

    pub fn format_sidebar_column_label(&self, col_idx: usize, name: &str) -> String {
        format!("{col_idx}: {name}")
    }

    fn column_slot_width(&self, col: usize) -> u16 {
        self.column_width_chars(col) as u16 + 1
    }

    pub fn max_visible_columns(&self, table_width: u16) -> usize {
        self.visible_column_range(table_width).len().max(1)
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
        if headers_len == 0 {
            return 0..0;
        }
        let start = self.view.col_offset.min(headers_len.saturating_sub(1));
        let mut used = 0u16;
        let mut end = start;
        while end < headers_len {
            let slot = self.column_slot_width(end);
            if used > 0 && used.saturating_add(slot) > table_width {
                break;
            }
            used = used.saturating_add(slot);
            end += 1;
        }
        if end == start {
            end = (start + 1).min(headers_len);
        }
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
                let kind = self.effective_column_kind(col_idx);
                Some(VisibleColumn {
                    index: col_idx,
                    name,
                    width: self.column_width_chars(col_idx) as u16,
                    kind,
                    align_right: is_right_aligned(kind),
                })
            })
            .collect();

        let sidebar_columns = headers
            .iter()
            .enumerate()
            .map(|(col_idx, name)| {
                SidebarColumn {
                    index: col_idx,
                    label: self.format_sidebar_column_label(col_idx, name),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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

    #[test]
    fn auto_fits_columns_on_open() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let model = AppModel::open(Some(path)).expect("open csv");
        assert!(model.column_width_chars(0) > MIN_COLUMN_WIDTH);
        assert!(model.column_width_chars(0) <= MAX_COLUMN_WIDTH);
    }

    #[test]
    fn manual_resize_locks_column() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.set_column_width(0, 10);
        assert!(model.view.column_widths_user_set[0]);
        let w = model.column_width_chars(0);
        model.refit_column_widths();
        assert_eq!(model.column_width_chars(0), w);
    }

    #[test]
    fn set_column_kind_changes_display() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.set_column_kind(0, ColumnKind::Text);
        assert_eq!(model.view.column_kinds[0], ColumnKind::Text);
    }
}
