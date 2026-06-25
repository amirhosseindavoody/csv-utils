use crate::column::{
    available_column_kinds, infer_kind_from_state, is_right_aligned, is_numeric,
    ColumnInferState, ColumnKind, NumericRepr,
};
use crate::column_stats::{build_column_info, ColumnInfo};
use crate::display::{format_cell_for_column, sanitize_ascii, truncate_middle};
use crate::settings::{self, normalize_decimal_format, SettingsFile};
use std::path::PathBuf;

pub const MIN_COLUMN_WIDTH: usize = 4;
pub const MAX_COLUMN_WIDTH: usize = 64;
const STATS_BACKFILL_BUDGET: usize = 512;

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
    /// Per-column decimal format override (`None` = use `csv-utils.json` default).
    pub column_decimal_formats: Vec<Option<String>>,
    pub show_column_info: bool,
    /// Focus index in column info panel (type, representation, decimal places).
    pub column_info_focus: usize,
    pub column_info_decimal_editing: bool,
    pub column_info_decimal_draft: String,
    pub show_help: bool,
}

#[derive(Debug)]
pub struct AppModel {
    pub file_path: Option<PathBuf>,
    pub preview: crate::preview::PreviewData,
    pub settings: SettingsFile,
    pub view: TableViewState,
    pub scan_thread: Option<std::thread::JoinHandle<()>>,
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
            column_decimal_formats: Vec::new(),
            show_column_info: false,
            column_info_focus: 0,
            column_info_decimal_editing: false,
            column_info_decimal_draft: String::new(),
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

impl AppModel {
    pub fn open(file_path: Option<PathBuf>) -> std::io::Result<Self> {
        let preview = match &file_path {
            Some(path) => crate::preview::PreviewData::load_header_and_initial_rows(
                path,
                crate::preview::INITIAL_BODY_LINES,
            )?,
            None => crate::preview::PreviewData::empty(),
        };

        let scan_thread = file_path.as_ref().and_then(|path| {
            if preview.scan_done() {
                None
            } else {
                let skip = preview.row_count();
                Some(preview.start_background_scan(path, skip))
            }
        });

        let settings = settings::load_or_create().unwrap_or_default();

        let mut model = Self {
            file_path,
            preview,
            settings,
            view: TableViewState::default(),
            scan_thread,
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

    pub fn reopen(&mut self, file_path: PathBuf) -> std::io::Result<()> {
        self.join_scan_thread();
        *self = Self::open(Some(file_path))?;
        Ok(())
    }

    pub fn close_file(&mut self) -> std::io::Result<()> {
        self.join_scan_thread();
        *self = Self::open(None)?;
        Ok(())
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
        if self.view.column_decimal_formats.len() != n {
            self.view.column_decimal_formats = vec![None; n];
        }
    }

    pub fn decimal_format_for_column(&self, col: usize) -> &str {
        self.view
            .column_decimal_formats
            .get(col)
            .and_then(|o| o.as_deref())
            .unwrap_or(&self.settings.display.numeric_decimal_format)
    }

    pub fn decimal_places_for_column(&self, col: usize) -> usize {
        settings::parse_decimal_format(self.decimal_format_for_column(col))
            .unwrap_or_else(settings::default_decimal_places)
    }

    pub fn set_column_decimal_format(&mut self, col: usize, format: String) {
        self.ensure_column_state();
        if col >= self.view.column_decimal_formats.len() {
            return;
        }
        if let Some(normalized) = normalize_decimal_format(&format) {
            self.view.column_decimal_formats[col] = Some(normalized);
            self.view.column_widths_user_set[col] = false;
            self.apply_fitted_column_width(col);
        }
    }

    pub fn column_width_chars(&self, col: usize) -> usize {
        self.view
            .column_widths
            .get(col)
            .copied()
            .unwrap_or(MIN_COLUMN_WIDTH as u16) as usize
    }

    fn layout(&self) -> std::sync::Arc<std::sync::Mutex<crate::column_layout::ColumnLayoutState>> {
        self.preview.layout()
    }

    fn apply_fitted_column_widths(&mut self) {
        let layout_arc = self.layout();
        let layout = layout_arc.lock().expect("layout mutex poisoned");
        let n = layout.column_count();
        for col in 0..n {
            if self
                .view
                .column_widths_user_set
                .get(col)
                .copied()
                .unwrap_or(false)
            {
                continue;
            }
            let w = layout
                .max_content_len(col)
                .clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH);
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
        let layout_arc = self.layout();
        let layout = layout_arc.lock().expect("layout mutex poisoned");
        let w = layout
            .max_content_len(col)
            .clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH);
        if col < self.view.column_widths.len() {
            self.view.column_widths[col] = w as u16;
        }
    }

    /// Apply auto-fit widths and incrementally backfill column stats when the info panel is open.
    pub fn maybe_update_column_layout(&mut self) {
        self.ensure_column_state();
        let headers = self.preview.headers();
        let n = headers.len();

        {
            let layout_arc = self.layout();
            let mut layout = layout_arc.lock().expect("layout mutex poisoned");
            if layout.column_count() != n {
                layout.reset_from_headers(&headers);
            }

            if let Some(col) = layout.stats_column() {
                let row_count = self.preview.row_count();
                let mut budget = STATS_BACKFILL_BUDGET;
                while layout.stats_backfill_row() < row_count && budget > 0 {
                    let row = layout.stats_backfill_row();
                    if let Some(fields) = self.preview.row_fields(row) {
                        layout.backfill_stats_for_row(col, &fields);
                    }
                    layout.advance_stats_backfill();
                    budget -= 1;
                }
            }
        }

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
        let layout_arc = self.layout();
        let layout = layout_arc.lock().expect("layout mutex poisoned");
        infer_kind_from_state(layout.infer_state(col))
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
        format_cell_for_column(text, width, kind, repr, self.decimal_places_for_column(col))
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

    pub fn column_infer_state(&self, col: usize) -> ColumnInferState {
        self.layout()
            .lock()
            .expect("layout mutex poisoned")
            .infer_state(col)
    }

    pub fn column_info_type_kinds(&self, col: usize) -> Vec<ColumnKind> {
        available_column_kinds(self.column_infer_state(col))
    }

    pub fn column_info_decimal_section_visible(&self, col: usize) -> bool {
        self.column_info_repr_section_visible(col)
    }

    pub fn column_info_decimal_focus_index(&self, col: usize) -> usize {
        self.column_info_type_kinds(col).len() + 2
    }

    pub fn open_column_info_pane(&mut self) {
        let col = self.view.selected_col;
        self.view.show_column_info = true;
        self.view.column_info_decimal_editing = false;
        self.view.column_info_decimal_draft.clear();
        self.layout()
            .lock()
            .expect("layout mutex poisoned")
            .set_stats_column(Some(col));
        let stored = self.stored_column_kind(col);
        let kinds = self.column_info_type_kinds(col);
        self.view.column_info_focus = kinds
            .iter()
            .position(|k| *k == stored)
            .unwrap_or(0);
    }

    pub fn close_column_info_pane(&mut self) {
        self.view.show_column_info = false;
        self.view.column_info_decimal_editing = false;
        self.view.column_info_decimal_draft.clear();
        self.layout()
            .lock()
            .expect("layout mutex poisoned")
            .set_stats_column(None);
    }

    fn column_info_kind_shows_repr(&self, col: usize, kind: ColumnKind) -> bool {
        match kind {
            ColumnKind::Int | ColumnKind::Float => true,
            ColumnKind::Auto => is_numeric(self.effective_column_kind(col)),
            _ => false,
        }
    }

    pub fn column_info_repr_section_visible(&self, col: usize) -> bool {
        if self
            .column_info_type_kinds(col)
            .iter()
            .any(|k| matches!(k, ColumnKind::Int | ColumnKind::Float))
        {
            return true;
        }
        self.column_info_kind_shows_repr(col, self.stored_column_kind(col))
    }

    pub fn column_info_repr_enabled(&self) -> bool {
        let col = self.view.selected_col;
        let kinds = self.column_info_type_kinds(col);
        let focus = self.view.column_info_focus;
        if focus < kinds.len() {
            return self.column_info_kind_shows_repr(col, kinds[focus]);
        }
        self.column_info_repr_section_visible(col)
    }

    pub fn column_info_focus_max(&self) -> usize {
        let col = self.view.selected_col;
        let type_count = self.column_info_type_kinds(col).len();
        if self.column_info_repr_section_visible(col) {
            type_count + 2
        } else {
            type_count.saturating_sub(1)
        }
    }

    pub fn column_info_focus_delta(&mut self, delta: i32) {
        if self.view.column_info_decimal_editing {
            return;
        }
        let max = self.column_info_focus_max() as i32;
        let next = self.view.column_info_focus as i32 + delta;
        self.view.column_info_focus = next.clamp(0, max) as usize;
    }

    pub fn column_info_start_decimal_edit(&mut self) {
        let col = self.view.selected_col;
        self.view.column_info_decimal_editing = true;
        self.view.column_info_decimal_draft = self.decimal_format_for_column(col).to_string();
        self.view.column_info_focus = self.column_info_decimal_focus_index(col);
    }

    pub fn column_info_apply_decimal_draft(&mut self) {
        let col = self.view.selected_col;
        let draft = self.view.column_info_decimal_draft.clone();
        self.set_column_decimal_format(col, draft);
        self.view.column_info_decimal_editing = false;
    }

    pub fn column_info_decimal_push_char(&mut self, ch: char) {
        if !self.view.column_info_decimal_editing {
            return;
        }
        if ch == '.' || ch.is_ascii_digit() {
            if ch == '.' && self.view.column_info_decimal_draft.contains('.') {
                return;
            }
            if self.view.column_info_decimal_draft.is_empty() && ch.is_ascii_digit() {
                self.view.column_info_decimal_draft.push('.');
            }
            self.view.column_info_decimal_draft.push(ch);
        }
    }

    pub fn column_info_decimal_backspace(&mut self) {
        if self.view.column_info_decimal_editing {
            self.view.column_info_decimal_draft.pop();
        }
    }

    pub fn column_info_apply_focus(&mut self) {
        let col = self.view.selected_col;
        let kinds = self.column_info_type_kinds(col);
        let focus = self.view.column_info_focus;
        let decimal_idx = self.column_info_decimal_focus_index(col);

        if focus == decimal_idx {
            if self.view.column_info_decimal_editing {
                self.column_info_apply_decimal_draft();
            } else {
                self.column_info_start_decimal_edit();
            }
            return;
        }

        if self.view.column_info_decimal_editing {
            self.view.column_info_decimal_editing = false;
            self.view.column_info_decimal_draft.clear();
        }

        if focus < kinds.len() {
            self.set_column_kind(col, kinds[focus]);
        } else {
            match focus - kinds.len() {
                0 => self.set_numeric_repr(col, NumericRepr::General),
                1 => self.set_numeric_repr(col, NumericRepr::Scientific),
                _ => {}
            }
        }
    }

    pub fn column_info_apply_option(&mut self, option: usize) {
        let max = self.column_info_focus_max();
        if option > max {
            return;
        }
        self.view.column_info_focus = option;
        let col = self.view.selected_col;
        if option == self.column_info_decimal_focus_index(col) {
            self.column_info_start_decimal_edit();
            return;
        }
        self.column_info_apply_focus();
    }

    pub fn stored_column_kind(&self, col: usize) -> ColumnKind {
        self.view
            .column_kinds
            .get(col)
            .copied()
            .unwrap_or(ColumnKind::Auto)
    }

    pub fn column_info(&self, col: usize) -> ColumnInfo {
        let headers = self.preview.headers();
        let name = headers.get(col).map(String::as_str).unwrap_or("");
        let stored = self.stored_column_kind(col);
        let effective = self.effective_column_kind(col);
        let repr = self.numeric_repr(col);
        let stats = self
            .layout()
            .lock()
            .expect("layout mutex poisoned")
            .stats(col);
        let decimal_visible = self.column_info_decimal_section_visible(col);
        let decimal_format = if decimal_visible {
            Some(self.decimal_format_for_column(col).to_string())
        } else {
            None
        };
        build_column_info(
            col,
            name,
            stored,
            effective,
            repr,
            &stats,
            self.preview.scan_done(),
            self.view.column_info_focus,
            self.column_info_repr_section_visible(col),
            self.column_info_repr_enabled(),
            &self.column_info_type_kinds(col),
            decimal_visible,
            decimal_format,
            self.view.column_info_decimal_editing,
            self.view.column_info_decimal_draft.clone(),
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
            if let Some(fields) = self.preview.row_fields(row_idx) {
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
