use crate::column::{
    available_column_kinds, infer_kind_from_state, is_right_aligned, is_numeric,
    ColumnInferState, ColumnKind, NumericRepr,
};
use crate::column_stats::{build_column_info, ColumnInfo};
use crate::display::{format_cell_for_column, sanitize_ascii, truncate_middle};
use crate::settings::{self, normalize_decimal_format, SettingsFile};
use crate::column_value_filter::{numeric_cell_matches, text_cell_matches, ColumnFilterError};
use crate::fuzzy::rank_by_fuzzy;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub const MIN_COLUMN_WIDTH: usize = 4;
pub const MAX_COLUMN_WIDTH: usize = 64;
/// Which axis **Space** toggles for multi-select (follows the last arrow-key navigation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MultiSelectAxis {
    #[default]
    Row,
    Column,
}

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
    /// Per-column decimal format override (`None` = use merged settings default).
    pub column_decimal_formats: Vec<Option<String>>,
    pub show_column_info: bool,
    /// Focus index in column info panel (type, representation, decimal places).
    pub column_info_focus: usize,
    pub column_info_decimal_editing: bool,
    pub column_info_decimal_draft: String,
    pub show_help: bool,
    /// One-character gap between table columns when true.
    pub show_column_borders: bool,
    /// Non-empty when the column sidebar lists only fuzzy-matched headers.
    pub column_name_filter: String,
    /// Per-column row value filter expression (`None` = no filter on that column).
    pub column_value_filters: Vec<Option<String>>,
    /// When true, `:filter` applies to the column sidebar instead of row values.
    pub column_sidebar_focused: bool,
    pub column_info_filter_editing: bool,
    pub column_info_filter_draft: String,
    /// Scroll offset (lines) for the column info panel content.
    pub column_info_scroll: u16,
    /// Cached result of the row-filter scan. `None` means stale — recompute on next access.
    pub(crate) cached_matching_rows: Option<Vec<usize>>,
    /// Row count at the time the cache was built, used to detect new rows arriving.
    pub(crate) cached_row_count: usize,
    /// Columns hidden from the table (still listed in the sidebar).
    pub column_hidden: Vec<bool>,
    /// Pinned column indices in chronological pin order (left-to-right in the table).
    pub column_pin_order: Vec<usize>,
    /// Multi-selection for bulk column actions (Ctrl+click). Empty means use `selected_col` only.
    pub multi_selected_cols: Vec<usize>,
    /// Rows hidden from the table (session-only).
    pub row_hidden: Vec<bool>,
    /// Pinned row indices in chronological pin order (top of the table).
    pub row_pin_order: Vec<usize>,
    /// Multi-selection for bulk row actions (Ctrl+click on table body). Empty means use `selected_row` only.
    pub multi_selected_rows: Vec<usize>,
    /// Anchor corner (row, col) for Ctrl+click / Ctrl+drag cell range selection.
    pub cell_range_anchor: Option<(usize, usize)>,
    /// Focus corner for cell range; inclusive rectangle with anchor.
    pub cell_range_focus: Option<(usize, usize)>,
    /// Last arrow navigation axis; **Space** toggles multi-select on this axis.
    pub last_multi_select_axis: MultiSelectAxis,
    /// Width of the column sidebar pane in terminal columns.
    pub column_sidebar_width: u16,
    /// When true, table scroll offsets are not pulled to follow the selected cell.
    pub table_scroll_decoupled: bool,
}

#[derive(Debug)]
pub struct AppModel {
    pub file_path: Option<PathBuf>,
    pub preview: crate::preview::PreviewData,
    pub settings: SettingsFile,
    pub view: TableViewState,
    pub scan_thread: Option<std::thread::JoinHandle<()>>,
    scan_cancel: Option<Arc<AtomicBool>>,
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
            show_column_borders: true,
            column_name_filter: String::new(),
            column_value_filters: Vec::new(),
            column_sidebar_focused: false,
            column_info_filter_editing: false,
            column_info_filter_draft: String::new(),
            column_info_scroll: 0,
            cached_matching_rows: None,
            cached_row_count: 0,
            column_hidden: Vec::new(),
            column_pin_order: Vec::new(),
            multi_selected_cols: Vec::new(),
            row_hidden: Vec::new(),
            row_pin_order: Vec::new(),
            multi_selected_rows: Vec::new(),
            cell_range_anchor: None,
            cell_range_focus: None,
            last_multi_select_axis: MultiSelectAxis::default(),
            column_sidebar_width: 32,
            table_scroll_decoupled: false,
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
    pub pinned: bool,
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
                let cancel = Arc::new(AtomicBool::new(false));
                let handle = preview.start_background_scan(path, skip, Arc::clone(&cancel));
                Some((handle, cancel))
            }
        });
        let (scan_thread, scan_cancel) = match scan_thread {
            Some((handle, cancel)) => (Some(handle), Some(cancel)),
            None => (None, None),
        };

        let settings = settings::load_or_create().unwrap_or_default();

        let mut model = Self {
            file_path,
            preview,
            settings: settings.clone(),
            view: TableViewState {
                show_column_borders: settings.display.show_column_borders,
                ..TableViewState::default()
            },
            scan_thread,
            scan_cancel,
        };
        model.ensure_column_state();
        model.maybe_update_column_layout();
        Ok(model)
    }

    fn request_scan_cancel(&self) {
        if let Some(cancel) = &self.scan_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn join_scan_thread(&mut self) {
        self.request_scan_cancel();
        if let Some(handle) = self.scan_thread.take() {
            let _ = handle.join();
        }
        self.scan_cancel = None;
    }

    /// Stop the background scan without waiting for it to finish (TUI shutdown).
    pub fn abandon_scan_thread(&mut self) {
        self.request_scan_cancel();
        self.scan_thread.take();
        self.scan_cancel = None;
    }

    pub fn reopen(&mut self, file_path: PathBuf) -> std::io::Result<()> {
        self.abandon_scan_thread();
        *self = Self::open(Some(file_path))?;
        Ok(())
    }

    pub fn close_file(&mut self) -> std::io::Result<()> {
        self.abandon_scan_thread();
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
        if self.view.column_value_filters.len() != n {
            self.view.column_value_filters = vec![None; n];
        }
        if self.view.column_hidden.len() != n {
            self.view.column_hidden = vec![false; n];
        }
        self.view.column_pin_order.retain(|&col| col < n);
    }

    pub fn is_column_hidden(&self, col: usize) -> bool {
        self.view.column_hidden.get(col).copied().unwrap_or(false)
    }

    pub fn is_column_pinned(&self, col: usize) -> bool {
        self.view.column_pin_order.contains(&col)
    }

    /// Non-hidden pinned column indices in chronological pin order (fixed left segment).
    pub fn pinned_table_columns(&self) -> Vec<usize> {
        self.view
            .column_pin_order
            .iter()
            .copied()
            .filter(|&col| !self.is_column_hidden(col))
            .collect()
    }

    /// Non-hidden unpinned column indices in header order (horizontally scrollable).
    pub fn scrollable_table_columns(&self) -> Vec<usize> {
        self.table_visible_columns()
            .into_iter()
            .filter(|&col| !self.is_column_pinned(col))
            .collect()
    }

    fn pinned_columns_width(&self) -> u16 {
        self.pinned_table_columns()
            .iter()
            .map(|&col| self.column_slot_width(col))
            .sum()
    }

    fn count_columns_fitting_width(&self, cols: &[usize], width: u16) -> usize {
        if cols.is_empty() {
            return 0;
        }
        let mut used = 0u16;
        let mut count = 0usize;
        for &col in cols {
            let slot = self.column_slot_width(col);
            if count > 0 && used.saturating_add(slot) > width {
                break;
            }
            used = used.saturating_add(slot);
            count += 1;
        }
        count.max(1)
    }

    pub fn scrollable_visible_count(&self, table_width: u16) -> usize {
        let pinned_width = self.pinned_columns_width();
        let remaining = table_width.saturating_sub(pinned_width);
        let scrollable = self.scrollable_table_columns();
        self.count_columns_fitting_width(&scrollable, remaining)
    }

    pub fn toggle_pin_selected_columns(&mut self) {
        self.ensure_column_state();
        for col in self.columns_for_bulk_action() {
            if self.is_column_pinned(col) {
                self.view.column_pin_order.retain(|&c| c != col);
            } else {
                self.view.column_pin_order.push(col);
            }
        }
        self.snap_col_offset_after_pin_change();
    }

    fn snap_col_offset_after_pin_change(&mut self) {
        let scrollable = self.scrollable_table_columns();
        let max = scrollable.len().saturating_sub(1);
        self.view.col_offset = self.view.col_offset.min(max);
    }

    /// Non-hidden column indices in header order (shown in the table).
    pub fn table_visible_columns(&self) -> Vec<usize> {
        let n = self.preview.headers().len();
        (0..n)
            .filter(|&col| !self.is_column_hidden(col))
            .collect()
    }

    pub fn is_column_multi_selected(&self, col: usize) -> bool {
        self.view.multi_selected_cols.contains(&col)
    }

    pub fn columns_for_bulk_action(&self) -> Vec<usize> {
        if self.view.multi_selected_cols.is_empty() {
            vec![self.view.selected_col]
        } else {
            self.view.multi_selected_cols.clone()
        }
    }

    pub fn select_column_click(&mut self, col: usize, extend: bool, column_list_height: usize) {
        self.couple_table_scroll_to_selection();
        self.set_multi_select_axis(MultiSelectAxis::Column);
        if extend {
            self.add_column_to_selection(col, column_list_height);
        } else {
            self.clear_cell_range();
            self.view.multi_selected_cols.clear();
            self.view.selected_col = col;
            if let Some(pos) = self.sidebar_position_of_column(col) {
                let height = column_list_height.max(1);
                if pos < self.view.column_list_offset {
                    self.view.column_list_offset = pos;
                } else if pos >= self.view.column_list_offset + height {
                    self.view.column_list_offset = pos.saturating_sub(height - 1);
                }
            }
            self.clamp_column_list_offset(column_list_height);
        }
    }

    /// Select a column, or add it to an existing selection (seeds the previously-selected
    /// column when building the first multi-select). Used by context-menu **Select** and
    /// Ctrl+click — never removes columns from the selection.
    pub fn add_column_to_selection(&mut self, col: usize, column_list_height: usize) {
        self.couple_table_scroll_to_selection();
        self.set_multi_select_axis(MultiSelectAxis::Column);
        self.clear_cell_range();
        self.view.multi_selected_rows.clear();
        if self.view.multi_selected_cols.is_empty() {
            let current = self.view.selected_col;
            if current != col {
                self.view.multi_selected_cols.push(current);
            }
        }
        if !self.is_column_multi_selected(col) {
            self.view.multi_selected_cols.push(col);
            self.view.multi_selected_cols.sort_unstable();
            self.view.multi_selected_cols.dedup();
        }
        self.view.selected_col = col;
        self.ensure_column_list_shows_selection(column_list_height);
    }

    /// Focus a column for a sidebar context-menu action without dropping bulk selection.
    pub fn focus_column_for_context_action(
        &mut self,
        col: usize,
        column_list_height: usize,
        single_select: bool,
    ) {
        self.couple_table_scroll_to_selection();
        self.set_multi_select_axis(MultiSelectAxis::Column);
        self.view.column_sidebar_focused = true;
        self.view.selected_col = col;
        if single_select {
            self.clear_cell_range();
            self.view.multi_selected_cols.clear();
        } else if !self.view.multi_selected_cols.is_empty() && !self.is_column_multi_selected(col) {
            self.view.multi_selected_cols.push(col);
        }
        self.ensure_column_list_shows_selection(column_list_height);
    }

    pub fn hide_selected_columns(&mut self) -> Result<(), &'static str> {
        self.ensure_column_state();
        let targets: Vec<usize> = self
            .columns_for_bulk_action()
            .into_iter()
            .filter(|&col| !self.is_column_hidden(col))
            .collect();
        if targets.is_empty() {
            return Ok(());
        }
        let visible_count = self.table_visible_columns().len();
        if targets.len() >= visible_count {
            return Err("Cannot hide every column");
        }
        for col in targets {
            if col < self.view.column_hidden.len() {
                self.view.column_hidden[col] = true;
            }
        }
        self.view
            .multi_selected_cols
            .retain(|&col| col < self.view.column_hidden.len() && !self.view.column_hidden[col]);
        self.snap_selection_after_column_visibility_change();
        Ok(())
    }

    pub fn unhide_selected_columns(&mut self) -> Result<(), &'static str> {
        self.ensure_column_state();
        let mut targets: Vec<usize> = self
            .columns_for_bulk_action()
            .into_iter()
            .filter(|&col| self.is_column_hidden(col))
            .collect();
        if targets.is_empty() {
            targets = (0..self.view.column_hidden.len())
                .filter(|&col| self.is_column_hidden(col))
                .collect();
        }
        for col in targets {
            self.view.column_hidden[col] = false;
        }
        Ok(())
    }

    fn ensure_row_hidden(&mut self) {
        let n = self.preview.row_count();
        if self.view.row_hidden.len() < n {
            self.view.row_hidden.resize(n, false);
        }
        self.view.row_pin_order.retain(|&row| row < n);
    }

    pub fn is_row_hidden(&self, row: usize) -> bool {
        self.view.row_hidden.get(row).copied().unwrap_or(false)
    }

    pub fn is_row_pinned(&self, row: usize) -> bool {
        self.view.row_pin_order.contains(&row)
    }

    /// Pinned rows in chronological pin order that are visible in the table (not hidden, pass filters).
    pub fn pinned_table_rows(&self, matching: &[usize]) -> Vec<usize> {
        self.view
            .row_pin_order
            .iter()
            .copied()
            .filter(|&row| matching.contains(&row))
            .collect()
    }

    /// Matching rows that are not pinned (horizontally scrollable segment below pinned rows).
    pub fn scrollable_table_rows(&self, matching: &[usize]) -> Vec<usize> {
        matching
            .iter()
            .copied()
            .filter(|&row| !self.is_row_pinned(row))
            .collect()
    }

    /// Row order for arrow-key navigation: pinned rows first (pin order), then scrollable rows.
    pub fn table_row_navigation_order(&self, matching: &[usize]) -> Vec<usize> {
        let mut order = self.pinned_table_rows(matching);
        order.extend(self.scrollable_table_rows(matching));
        order
    }

    pub fn scrollable_row_visible_count(&self, viewport_rows: usize) -> usize {
        self.visible_table_rows(viewport_rows)
            .iter()
            .filter(|&&row| !self.is_row_pinned(row))
            .count()
            .max(1)
    }

    pub fn toggle_pin_selected_rows(&mut self) {
        self.ensure_row_hidden();
        for row in self.rows_for_bulk_action() {
            if self.is_row_pinned(row) {
                self.view.row_pin_order.retain(|&r| r != row);
            } else {
                self.view.row_pin_order.push(row);
            }
        }
        self.snap_row_offset_after_pin_change();
    }

    fn snap_row_offset_after_pin_change(&mut self) {
        let empty: &[usize] = &[];
        let matching = self.view.cached_matching_rows.as_deref().unwrap_or(empty);
        let scrollable = self.scrollable_table_rows(matching);
        let max = scrollable.len().saturating_sub(1);
        self.view.row_offset = self.view.row_offset.min(max);
    }

    /// Row indices shown in the table viewport (pinned first, then scrollable window).
    pub fn visible_table_rows(&self, viewport_rows: usize) -> Vec<usize> {
        let matching = self.matching_rows_for_display();
        if matching.is_empty() {
            return Vec::new();
        }

        let pinned = self.pinned_table_rows(&matching);
        let scrollable = self.scrollable_table_rows(&matching);
        let mut result = pinned;

        if scrollable.is_empty() {
            if result.is_empty() {
                result.push(matching[0]);
            }
            return result;
        }

        let start_idx = self.view.row_offset.min(scrollable.len().saturating_sub(1));
        for &row in &scrollable[start_idx..] {
            if !result.is_empty() && result.len() >= viewport_rows {
                break;
            }
            result.push(row);
        }

        if result.is_empty() {
            result.push(
                self.pinned_table_rows(&matching)
                    .first()
                    .copied()
                    .unwrap_or(scrollable[start_idx]),
            );
        }

        result
    }

    fn matching_rows_for_display(&self) -> Vec<usize> {
        if let Some(m) = self.cached_matching_rows() {
            return m.to_vec();
        }
        let current_count = self.preview.row_count();
        (0..current_count)
            .filter(|&row| {
                !self.is_row_hidden(row)
                    && (!self.row_value_filters_active() || self.row_passes_value_filters(row))
            })
            .collect()
    }

    pub fn rows_hidden_active(&self) -> bool {
        self.view.row_hidden.iter().any(|&hidden| hidden)
    }

    pub fn is_row_multi_selected(&self, row: usize) -> bool {
        self.view.multi_selected_rows.contains(&row)
    }

    pub fn cell_range_active(&self) -> bool {
        self.view.cell_range_anchor.is_some()
    }

    fn cell_range_bounds(&self) -> Option<(usize, usize, usize, usize)> {
        let anchor = self.view.cell_range_anchor?;
        let focus = self.view.cell_range_focus?;
        let (r0, c0) = anchor;
        let (r1, c1) = focus;
        Some((r0.min(r1), r0.max(r1), c0.min(c1), c0.max(c1)))
    }

    pub fn is_cell_in_selection_range(&self, row: usize, col: usize) -> bool {
        let Some((min_r, max_r, min_c, max_c)) = self.cell_range_bounds() else {
            return false;
        };
        row >= min_r && row <= max_r && col >= min_c && col <= max_c
    }

    pub fn row_in_cell_range_row_span(&self, row: usize) -> bool {
        let Some((min_r, max_r, _, _)) = self.cell_range_bounds() else {
            return false;
        };
        row >= min_r && row <= max_r
    }

    pub fn clear_cell_range(&mut self) {
        self.view.cell_range_anchor = None;
        self.view.cell_range_focus = None;
    }

    /// Returns the anchor for an in-progress range, creating one at `(row, col)` when needed.
    pub fn begin_cell_range_if_needed(&mut self, row: usize, col: usize) -> (usize, usize) {
        self.view.multi_selected_cols.clear();
        self.view.multi_selected_rows.clear();
        if let Some(anchor) = self.view.cell_range_anchor {
            anchor
        } else {
            self.view.cell_range_anchor = Some((row, col));
            self.view.cell_range_focus = Some((row, col));
            (row, col)
        }
    }

    pub fn set_cell_range_focus(&mut self, row: usize, col: usize) {
        if self.view.cell_range_anchor.is_some() {
            self.view.cell_range_focus = Some((row, col));
        }
    }

    pub fn set_cell_range_corners(
        &mut self,
        anchor_row: usize,
        anchor_col: usize,
        focus_row: usize,
        focus_col: usize,
    ) {
        self.view.multi_selected_cols.clear();
        self.view.multi_selected_rows.clear();
        self.view.cell_range_anchor = Some((anchor_row, anchor_col));
        self.view.cell_range_focus = Some((focus_row, focus_col));
    }

    pub fn rows_for_bulk_action(&self) -> Vec<usize> {
        if let Some((min_r, max_r, _, _)) = self.cell_range_bounds() {
            return (min_r..=max_r).collect();
        }
        if self.view.multi_selected_rows.is_empty() {
            vec![self.view.selected_row]
        } else {
            self.view.multi_selected_rows.clone()
        }
    }

    fn toggle_sorted(vec: &mut Vec<usize>, item: usize) {
        if let Some(pos) = vec.iter().position(|&x| x == item) {
            vec.remove(pos);
        } else {
            vec.push(item);
            vec.sort_unstable();
            vec.dedup();
        }
    }

    pub fn toggle_row_multi_select(&mut self, row: usize) {
        self.clear_cell_range();
        self.view.multi_selected_cols.clear();
        Self::toggle_sorted(&mut self.view.multi_selected_rows, row);
    }

    pub fn toggle_column_multi_select(&mut self, col: usize) {
        self.clear_cell_range();
        self.view.multi_selected_rows.clear();
        Self::toggle_sorted(&mut self.view.multi_selected_cols, col);
    }

    pub fn toggle_multi_select_at_focus(&mut self) {
        match self.view.last_multi_select_axis {
            MultiSelectAxis::Row => {
                let row = self.view.selected_row;
                self.toggle_row_multi_select(row);
            }
            MultiSelectAxis::Column => {
                let col = self.view.selected_col;
                self.toggle_column_multi_select(col);
            }
        }
    }

    pub fn set_multi_select_axis(&mut self, axis: MultiSelectAxis) {
        self.view.last_multi_select_axis = axis;
    }

    pub fn select_table_cell_click(
        &mut self,
        row: usize,
        col: usize,
        extend: bool,
        column_list_height: usize,
    ) {
        self.couple_table_scroll_to_selection();
        self.set_multi_select_axis(MultiSelectAxis::Row);
        self.view.selected_row = row;
        self.view.selected_col = col;
        if extend {
            self.begin_cell_range_if_needed(row, col);
            self.set_cell_range_focus(row, col);
        } else {
            self.clear_cell_range();
            self.view.multi_selected_rows.clear();
        }
        self.ensure_column_list_shows_selection(column_list_height);
    }

    pub fn select_table_header_click(&mut self, col: usize, extend: bool, column_list_height: usize) {
        self.couple_table_scroll_to_selection();
        self.set_multi_select_axis(MultiSelectAxis::Column);
        if extend {
            self.add_column_to_selection(col, column_list_height);
        } else {
            self.clear_cell_range();
            self.view.multi_selected_cols.clear();
            self.view.selected_col = col;
            self.ensure_column_list_shows_selection(column_list_height);
        }
    }

    pub fn hide_selected_rows(&mut self) -> Result<(), &'static str> {
        self.ensure_row_hidden();
        let targets: Vec<usize> = self
            .rows_for_bulk_action()
            .into_iter()
            .filter(|&row| !self.is_row_hidden(row))
            .collect();
        if targets.is_empty() {
            return Ok(());
        }
        let visible_count = (0..self.preview.row_count())
            .filter(|&row| !self.is_row_hidden(row))
            .count();
        if targets.len() >= visible_count {
            return Err("Cannot hide every row");
        }
        for row in targets {
            if row < self.view.row_hidden.len() {
                self.view.row_hidden[row] = true;
            }
        }
        self.view.multi_selected_rows.retain(|&row| {
            row < self.view.row_hidden.len() && !self.view.row_hidden[row]
        });
        self.view.cell_range_anchor = self.view.cell_range_anchor.filter(|&(r, _)| {
            r < self.view.row_hidden.len() && !self.view.row_hidden[r]
        });
        if self.view.cell_range_anchor.is_none() {
            self.view.cell_range_focus = None;
        }
        self.invalidate_row_cache();
        self.snap_selection_to_visible_rows();
        Ok(())
    }

    pub fn unhide_selected_rows(&mut self) -> Result<(), &'static str> {
        self.ensure_row_hidden();
        let mut targets: Vec<usize> = self
            .rows_for_bulk_action()
            .into_iter()
            .filter(|&row| self.is_row_hidden(row))
            .collect();
        if targets.is_empty() {
            targets = (0..self.preview.row_count())
                .filter(|&row| self.is_row_hidden(row))
                .collect();
        }
        for row in targets {
            if row < self.view.row_hidden.len() {
                self.view.row_hidden[row] = false;
            }
        }
        self.invalidate_row_cache();
        Ok(())
    }

    /// Hide columns when the sidebar is focused or the last arrow axis was column;
    /// otherwise hide rows (including cell-range spans).
    pub fn hide_from_command(&mut self) -> Result<(), &'static str> {
        if self.hide_command_targets_columns() {
            self.hide_selected_columns()
        } else {
            self.hide_selected_rows()
        }
    }

    /// Unhide columns when the sidebar is focused or the last arrow axis was column;
    /// otherwise unhide rows.
    pub fn unhide_from_command(&mut self) -> Result<(), &'static str> {
        if self.hide_command_targets_columns() {
            self.unhide_selected_columns()
        } else {
            self.unhide_selected_rows()
        }
    }

    fn hide_command_targets_columns(&self) -> bool {
        self.view.column_sidebar_focused
            || self.view.last_multi_select_axis == MultiSelectAxis::Column
    }

    /// Move column selection by `delta`, skipping hidden columns.
    pub fn move_selected_column(&mut self, delta: i32, column_list_height: usize) {
        self.couple_table_scroll_to_selection();
        let n = self.preview.headers().len();
        if n == 0 {
            return;
        }
        let step = if delta < 0 { -1i32 } else { 1 };
        let mut col = self.view.selected_col;
        loop {
            let next = if step < 0 {
                col.checked_sub(1)
            } else if col + 1 < n {
                Some(col + 1)
            } else {
                None
            };
            let Some(next) = next else {
                break;
            };
            col = next;
            if !self.is_column_hidden(col) {
                self.view.selected_col = col;
                break;
            }
        }
        self.ensure_column_list_shows_selection(column_list_height);
    }

    fn snap_selection_after_column_visibility_change(&mut self) {
        let table_cols = self.table_visible_columns();
        if table_cols.is_empty() {
            return;
        }
        if !table_cols.contains(&self.view.selected_col) {
            self.view.selected_col = table_cols[0];
        }
        let scrollable = self.scrollable_table_columns();
        let max_offset = scrollable.len().saturating_sub(1);
        self.view.col_offset = self.view.col_offset.min(max_offset);
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

    /// Apply auto-fit widths from the shared column layout state.
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
        }

        self.apply_fitted_column_widths();
        // Warm the row-filter cache so draw code can read it without recomputing every frame.
        self.matching_row_indices();
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
        let label = if self.column_has_value_filter(col) {
            format!("{name}*")
        } else {
            name.to_string()
        };
        let visible = sanitize_ascii(&label);
        if visible.len() <= width {
            format!("{visible:<width$}", width = width)
        } else {
            format!("{:<width$}", truncate_middle(&label, width), width = width)
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
        self.view.column_info_filter_editing = false;
        self.view.column_info_filter_draft.clear();
        self.view.column_info_scroll = 0;
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
        self.view.column_info_filter_editing = false;
        self.view.column_info_filter_draft.clear();
        self.view.column_info_scroll = 0;
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
        self.column_info_filter_focus_index(self.view.selected_col)
    }

    pub fn column_info_focus_delta(&mut self, delta: i32) {
        if self.view.column_info_decimal_editing || self.view.column_info_filter_editing {
            return;
        }
        let max = self.column_info_focus_max() as i32;
        let next = self.view.column_info_focus as i32 + delta;
        self.view.column_info_focus = next.clamp(0, max) as usize;
    }

    /// Line index in the column info panel content for the current focus item.
    pub fn column_info_focus_content_line(&self, col: usize) -> usize {
        let focus = self.view.column_info_focus;
        let type_count = self.column_info_type_kinds(col).len();
        if focus < type_count {
            return 3 + focus;
        }
        if self.column_info_repr_section_visible(col) {
            if focus < type_count + 2 {
                return 3 + type_count + 2 + (focus - type_count);
            }
            if focus == type_count + 2 {
                return 3 + type_count + 6;
            }
            return 3 + type_count + 9;
        }
        3 + type_count + 2
    }

    pub fn column_info_scroll_by(&mut self, delta: i32) {
        let next = self.view.column_info_scroll as i32 + delta;
        self.view.column_info_scroll = next.max(0) as u16;
    }

    pub fn column_info_clamp_scroll(&mut self, viewport: u16, total_lines: usize) {
        if viewport == 0 {
            return;
        }
        let max_scroll = total_lines.saturating_sub(viewport as usize) as u16;
        if self.view.column_info_scroll > max_scroll {
            self.view.column_info_scroll = max_scroll;
        }
    }

    pub fn column_info_ensure_focus_visible(&mut self, col: usize, viewport: u16) {
        if viewport == 0 {
            return;
        }
        let line = self.column_info_focus_content_line(col);
        let scroll = self.view.column_info_scroll as usize;
        let view = viewport as usize;
        if line < scroll {
            self.view.column_info_scroll = line as u16;
        } else if line >= scroll + view {
            self.view.column_info_scroll = (line + 1).saturating_sub(view) as u16;
        }
    }

    pub fn max_row_offset(&mut self, viewport_rows: usize) -> usize {
        self.matching_row_indices();
        let matching = self
            .view
            .cached_matching_rows
            .as_ref()
            .map(|m| m.as_slice())
            .unwrap_or(&[]);
        let scrollable = self.scrollable_table_rows(matching);
        if scrollable.is_empty() {
            return 0;
        }
        let pinned_count = self.pinned_table_rows(matching).len();
        let scrollable_visible = viewport_rows.saturating_sub(pinned_count).max(1);
        scrollable.len().saturating_sub(scrollable_visible)
    }

    pub fn max_col_offset(&self, table_width: u16) -> usize {
        let scrollable = self.scrollable_table_columns();
        if scrollable.is_empty() {
            return 0;
        }
        let pinned_width = self.pinned_columns_width();
        let remaining = table_width.saturating_sub(pinned_width);
        let max_visible = self.count_columns_fitting_width(&scrollable, remaining);
        scrollable.len().saturating_sub(max_visible)
    }

    pub fn set_row_offset(&mut self, offset: usize, viewport_rows: usize) {
        self.view.table_scroll_decoupled = true;
        let max = self.max_row_offset(viewport_rows);
        self.view.row_offset = offset.min(max);
    }

    pub fn set_col_offset(&mut self, offset: usize, table_width: u16) {
        self.view.table_scroll_decoupled = true;
        let max = self.max_col_offset(table_width);
        self.view.col_offset = offset.min(max);
    }

    pub fn couple_table_scroll_to_selection(&mut self) {
        self.view.table_scroll_decoupled = false;
    }

    pub fn set_column_list_scroll(&mut self, offset: usize, visible_height: usize) {
        self.view.column_list_offset = offset;
        self.clamp_column_list_offset(visible_height);
    }

    pub fn set_column_info_scroll_position(
        &mut self,
        scroll: usize,
        viewport: u16,
        total_lines: usize,
    ) {
        self.view.column_info_scroll = scroll as u16;
        self.column_info_clamp_scroll(viewport, total_lines);
    }

    pub fn column_info_content_line_count(&self, col: usize) -> usize {
        let type_count = self.column_info_type_kinds(col).len();
        let repr = self.column_info_repr_section_visible(col);
        let stat_count = self.column_info(col).stats.len();
        let filter_line = if repr {
            3 + type_count + 9
        } else {
            3 + type_count + 2
        };
        filter_line + stat_count + 5
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
        let filter_idx = self.column_info_filter_focus_index(col);

        if focus == filter_idx {
            if self.view.column_info_filter_editing {
                let _ = self.column_info_apply_filter_draft();
            } else {
                self.column_info_start_filter_edit();
            }
            return;
        }

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
        if self.view.column_info_filter_editing {
            self.view.column_info_filter_editing = false;
            self.view.column_info_filter_draft.clear();
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
        if option == self.column_info_filter_focus_index(col) {
            self.column_info_start_filter_edit();
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
        let star = if self.column_has_value_filter(col_idx) {
            "*"
        } else {
            ""
        };
        format!("{col_idx}: {name}{star}")
    }

    pub fn column_has_value_filter(&self, col: usize) -> bool {
        self.view
            .column_value_filters
            .get(col)
            .and_then(|f| f.as_ref())
            .is_some_and(|s| !s.trim().is_empty())
    }

    pub fn column_value_filter_display(&self, col: usize) -> Option<&str> {
        self.view
            .column_value_filters
            .get(col)
            .and_then(|f| f.as_deref())
            .filter(|s| !s.trim().is_empty())
    }

    pub fn column_value_filter_is_numeric(&self, col: usize) -> bool {
        is_numeric(self.effective_column_kind(col))
    }

    pub fn row_value_filters_active(&self) -> bool {
        self.view
            .column_value_filters
            .iter()
            .any(|f| f.as_ref().is_some_and(|s| !s.trim().is_empty()))
    }

    pub fn set_column_value_filter(
        &mut self,
        col: usize,
        expr: String,
    ) -> Result<(), ColumnFilterError> {
        self.ensure_column_state();
        if col >= self.view.column_value_filters.len() {
            return Ok(());
        }
        let trimmed = expr.trim().to_string();
        if trimmed.is_empty() {
            self.view.column_value_filters[col] = None;
        } else if self.column_value_filter_is_numeric(col) {
            crate::column_value_filter::validate_numeric_filter(&trimmed)?;
            self.view.column_value_filters[col] = Some(trimmed);
        } else {
            self.view.column_value_filters[col] = Some(trimmed);
        }
        self.invalidate_row_cache();
        self.snap_selection_to_visible_rows();
        Ok(())
    }

    pub fn clear_column_value_filter(&mut self, col: usize) {
        self.ensure_column_state();
        if col < self.view.column_value_filters.len() {
            self.view.column_value_filters[col] = None;
            self.invalidate_row_cache();
            self.snap_selection_to_visible_rows();
        }
    }

    fn cell_matches_value_filter(
        &self,
        col: usize,
        cell: &str,
        expr: &str,
    ) -> Result<bool, ColumnFilterError> {
        if self.column_value_filter_is_numeric(col) {
            numeric_cell_matches(cell, expr)
        } else {
            Ok(text_cell_matches(cell, expr))
        }
    }

    fn row_passes_value_filters(&self, row: usize) -> bool {
        let Some(fields) = self.preview.row_fields(row) else {
            return false;
        };
        for (col, filter) in self.view.column_value_filters.iter().enumerate() {
            let Some(expr) = filter.as_ref().filter(|s| !s.trim().is_empty()) else {
                continue;
            };
            let cell = fields.get(col).map(String::as_str).unwrap_or("");
            match self.cell_matches_value_filter(col, cell, expr) {
                Ok(true) => {}
                Ok(false) | Err(_) => return false,
            }
        }
        true
    }

    fn invalidate_row_cache(&mut self) {
        self.view.cached_matching_rows = None;
    }

    /// Return the cached matching rows without recomputing (may be stale if not yet ticked).
    /// Call `matching_row_indices` (mut) to ensure freshness; use this in draw code only.
    pub fn cached_matching_rows(&self) -> Option<&[usize]> {
        self.view.cached_matching_rows.as_deref()
    }

    /// Rebuild the cache if stale (filters changed or new rows arrived), then return a slice.
    pub fn matching_row_indices(&mut self) -> &[usize] {
        self.ensure_row_hidden();
        let current_count = self.preview.row_count();
        let cache_valid = self.view.cached_matching_rows.is_some()
            && self.view.cached_row_count == current_count;
        if !cache_valid {
            let rows: Vec<usize> = (0..current_count)
                .filter(|&row| {
                    !self.is_row_hidden(row)
                        && (!self.row_value_filters_active() || self.row_passes_value_filters(row))
                })
                .collect();
            self.view.cached_matching_rows = Some(rows);
            self.view.cached_row_count = current_count;
        }
        self.view.cached_matching_rows.as_deref().unwrap()
    }

    fn snap_selection_to_visible_rows(&mut self) {
        self.matching_row_indices(); // warm cache
        let sel = self.view.selected_row;
        let m = self.view.cached_matching_rows.as_deref().unwrap();
        let (first, contains) = (m.first().copied(), m.contains(&sel));
        if let Some(first) = first {
            if !contains {
                self.view.selected_row = first;
            }
        }
    }

    pub fn move_selected_row(&mut self, delta: i32) {
        self.couple_table_scroll_to_selection();
        self.matching_row_indices();
        let sel = self.view.selected_row;
        let m = self.view.cached_matching_rows.as_deref().unwrap();
        let order = self.table_row_navigation_order(m);
        if order.is_empty() {
            return;
        }
        let pos = order.iter().position(|&r| r == sel).unwrap_or(0);
        let next = ((pos as i32) + delta).clamp(0, order.len() as i32 - 1) as usize;
        self.view.selected_row = order[next];
    }

    pub fn first_navigation_row(&mut self) -> Option<usize> {
        self.matching_row_indices();
        let m = self.view.cached_matching_rows.as_deref()?;
        self.table_row_navigation_order(m).first().copied()
    }

    pub fn last_navigation_row(&mut self) -> Option<usize> {
        self.matching_row_indices();
        let m = self.view.cached_matching_rows.as_deref()?;
        self.table_row_navigation_order(m).last().copied()
    }

    /// Select a row, or add it to an existing selection (seeds the previously-selected row
    /// when building the first multi-select). Used by context-menu **Select** and Ctrl+click.
    pub fn add_row_to_selection(&mut self, row: usize) {
        self.couple_table_scroll_to_selection();
        self.set_multi_select_axis(MultiSelectAxis::Row);
        self.view.column_sidebar_focused = false;
        self.clear_cell_range();
        self.view.multi_selected_cols.clear();
        if self.view.multi_selected_rows.is_empty() {
            let current = self.view.selected_row;
            if current != row {
                self.view.multi_selected_rows.push(current);
            }
        }
        if !self.is_row_multi_selected(row) {
            self.view.multi_selected_rows.push(row);
            self.view.multi_selected_rows.sort_unstable();
            self.view.multi_selected_rows.dedup();
        }
        self.view.selected_row = row;
    }

    /// Focus a row for a gutter context-menu action without dropping bulk selection.
    pub fn focus_row_for_context_action(
        &mut self,
        row: usize,
        single_select: bool,
    ) {
        self.couple_table_scroll_to_selection();
        self.set_multi_select_axis(MultiSelectAxis::Row);
        self.view.column_sidebar_focused = false;
        self.view.selected_row = row;
        if single_select {
            self.clear_cell_range();
            self.view.multi_selected_rows.clear();
        } else if !self.view.multi_selected_rows.is_empty() && !self.is_row_multi_selected(row) {
            self.view.multi_selected_rows.push(row);
        }
    }

    /// Move selection up/down in the filtered column sidebar list.
    pub fn move_selected_sidebar_column(&mut self, delta: i32, visible_height: usize) {
        let filtered = self.filtered_sidebar_columns();
        if filtered.is_empty() {
            return;
        }
        let pos = self
            .sidebar_position_of_column(self.view.selected_col)
            .unwrap_or(0);
        let next = ((pos as i32) + delta).clamp(0, filtered.len() as i32 - 1) as usize;
        let col = filtered[next];
        self.select_sidebar_column(col, visible_height);
    }

    pub fn column_info_filter_focus_index(&self, col: usize) -> usize {
        let type_count = self.column_info_type_kinds(col).len();
        if self.column_info_repr_section_visible(col) {
            type_count + 3
        } else {
            type_count
        }
    }

    pub fn column_info_start_filter_edit(&mut self) {
        let col = self.view.selected_col;
        self.view.column_info_filter_editing = true;
        self.view.column_info_filter_draft = self
            .column_value_filter_display(col)
            .unwrap_or("")
            .to_string();
        self.view.column_info_focus = self.column_info_filter_focus_index(col);
    }

    pub fn column_info_apply_filter_draft(&mut self) -> Result<(), ColumnFilterError> {
        let col = self.view.selected_col;
        let draft = self.view.column_info_filter_draft.clone();
        self.set_column_value_filter(col, draft)?;
        self.view.column_info_filter_editing = false;
        Ok(())
    }

    pub fn column_info_filter_push_char(&mut self, ch: char) {
        if self.view.column_info_filter_editing && !ch.is_ascii_control() {
            self.view.column_info_filter_draft.push(ch);
        }
    }

    pub fn column_info_filter_backspace(&mut self) {
        if self.view.column_info_filter_editing {
            self.view.column_info_filter_draft.pop();
        }
    }

    pub fn set_column_name_filter(&mut self, filter: String) {
        self.view.column_name_filter = filter;
        self.view.column_list_offset = 0;
        let filtered = self.filtered_sidebar_columns();
        if !filtered.is_empty() && !filtered.contains(&self.view.selected_col) {
            self.view.selected_col = filtered[0];
        }
    }

    pub fn clear_column_name_filter(&mut self) {
        self.view.column_name_filter.clear();
        self.view.column_list_offset = 0;
    }

    pub fn column_name_filter_active(&self) -> bool {
        !self.view.column_name_filter.is_empty()
    }

    /// Sidebar column indices for a name filter (empty shows all columns).
    pub fn sidebar_columns_for_filter(&self, filter: &str) -> Vec<usize> {
        let headers = self.preview.headers();
        if headers.is_empty() {
            return Vec::new();
        }
        let query = filter.trim();
        if query.is_empty() {
            return (0..headers.len()).collect();
        }
        if query.bytes().all(|b| b.is_ascii_digit()) {
            if let Ok(idx) = query.parse::<usize>() {
                if idx < headers.len() {
                    return vec![idx];
                }
            }
        }
        rank_by_fuzzy(
            query,
            headers
                .iter()
                .enumerate()
                .map(|(idx, name)| (idx, name.as_str())),
        )
    }

    /// Sidebar column indices after applying `column_name_filter`.
    /// Pinned columns are listed first in chronological pin order.
    pub fn filtered_sidebar_columns(&self) -> Vec<usize> {
        self.order_sidebar_columns(self.sidebar_columns_for_filter(
            &self.view.column_name_filter,
        ))
    }

    fn order_sidebar_columns(&self, cols: Vec<usize>) -> Vec<usize> {
        let mut ordered = Vec::with_capacity(cols.len());
        for &col in &self.view.column_pin_order {
            if cols.contains(&col) && !self.is_column_hidden(col) {
                ordered.push(col);
            }
        }
        ordered.extend(
            cols.iter()
                .copied()
                .filter(|c| !self.is_column_pinned(*c) && !self.is_column_hidden(*c)),
        );
        ordered.extend(cols.iter().copied().filter(|c| self.is_column_hidden(*c)));
        ordered
    }

    fn sidebar_position_of_column(&self, col: usize) -> Option<usize> {
        self.filtered_sidebar_columns().iter().position(|&c| c == col)
    }

    pub fn select_sidebar_column(&mut self, col: usize, visible_height: usize) {
        self.select_column_click(col, false, visible_height);
    }

    fn column_slot_width(&self, col: usize) -> u16 {
        self.column_width_chars(col) as u16 + self.column_separator_width()
    }

    pub fn column_separator_width(&self) -> u16 {
        1
    }

    pub fn toggle_column_borders(&mut self) {
        self.view.show_column_borders = !self.view.show_column_borders;
    }

    pub fn max_visible_columns(&self, table_width: u16) -> usize {
        self.visible_table_columns(table_width).len().max(1)
    }

    pub fn clamp_selection(&mut self, viewport_rows: usize, table_width: u16) {
        // Build the cache first, then extract what we need as plain values.
        self.matching_row_indices();
        let sel = self.view.selected_row;
        let m = self.view.cached_matching_rows.as_deref().unwrap();
        let first_row = m.first().copied();
        let contains_sel = m.contains(&sel);
        let selected_pos = m.iter().position(|&r| r == sel).unwrap_or(0);
        let match_len = m.len();
        let max_cols = self.preview.headers().len().saturating_sub(1);
        let table_cols = self.table_visible_columns();
        let pinned_row_count = self.pinned_table_rows(m).len();
        let scrollable_rows = self.scrollable_table_rows(m);
        let scrollable_row_visible = viewport_rows.saturating_sub(pinned_row_count).max(1);

        self.view.selected_col = self.view.selected_col.min(max_cols);

        if let Some(first) = first_row {
            if !contains_sel {
                self.view.selected_row = first;
            }
        }

        let max_offset = scrollable_rows.len().saturating_sub(scrollable_row_visible);
        if !self.view.table_scroll_decoupled && !self.is_row_pinned(self.view.selected_row) {
            if let Some(sel_pos) = scrollable_rows
                .iter()
                .position(|&r| r == self.view.selected_row)
            {
                if sel_pos < self.view.row_offset {
                    self.view.row_offset = sel_pos;
                } else if scrollable_row_visible > 0
                    && sel_pos >= self.view.row_offset + scrollable_row_visible
                {
                    self.view.row_offset = sel_pos.saturating_sub(scrollable_row_visible - 1);
                }
            }
        }
        self.view.row_offset = self.view.row_offset.min(max_offset);
        let _ = (match_len, selected_pos);

        if !table_cols.is_empty() {
            let scrollable = self.scrollable_table_columns();
            let max_col_offset = if self.view.table_scroll_decoupled {
                self.max_col_offset(table_width)
            } else {
                scrollable.len().saturating_sub(1)
            };
            self.view.col_offset = self.view.col_offset.min(max_col_offset);
            if !self.view.table_scroll_decoupled && !self.is_column_pinned(self.view.selected_col) {
                let scrollable_visible = self.scrollable_visible_count(table_width);
                if let Some(sel_pos) = scrollable
                    .iter()
                    .position(|&c| c == self.view.selected_col)
                {
                    if sel_pos < self.view.col_offset {
                        self.view.col_offset = sel_pos;
                    } else if scrollable_visible > 0
                        && sel_pos >= self.view.col_offset + scrollable_visible
                    {
                        self.view.col_offset = sel_pos.saturating_sub(scrollable_visible - 1);
                    }
                }
            }
        }
    }

    pub fn clamp_column_list_offset(&mut self, visible_height: usize) {
        let count = self.filtered_sidebar_columns().len();
        let max_offset = count.saturating_sub(visible_height.max(1));
        self.view.column_list_offset = self.view.column_list_offset.min(max_offset);
    }

    pub fn ensure_column_list_shows_selection(&mut self, visible_height: usize) {
        let height = visible_height.max(1);
        if height == usize::MAX {
            return;
        }
        let Some(pos) = self.sidebar_position_of_column(self.view.selected_col) else {
            return;
        };
        if pos < self.view.column_list_offset {
            self.view.column_list_offset = pos;
        } else if pos >= self.view.column_list_offset + height {
            self.view.column_list_offset = pos.saturating_sub(height - 1);
        }
        self.clamp_column_list_offset(height);
    }

    /// Column indices shown in the table viewport (pinned first, then scrollable window).
    pub fn visible_table_columns(&self, table_width: u16) -> Vec<usize> {
        let table_cols = self.table_visible_columns();
        if table_cols.is_empty() {
            return Vec::new();
        }

        let pinned = self.pinned_table_columns();
        let scrollable = self.scrollable_table_columns();

        let mut used = 0u16;
        let mut result = Vec::new();

        for &col in &pinned {
            let slot = self.column_slot_width(col);
            if !result.is_empty() && used.saturating_add(slot) > table_width {
                break;
            }
            used = used.saturating_add(slot);
            result.push(col);
        }

        if scrollable.is_empty() {
            if result.is_empty() {
                result.push(table_cols[0]);
            }
            return result;
        }

        let start_idx = self.view.col_offset.min(scrollable.len().saturating_sub(1));
        for &col in &scrollable[start_idx..] {
            let slot = self.column_slot_width(col);
            if !result.is_empty() && used.saturating_add(slot) > table_width {
                break;
            }
            used = used.saturating_add(slot);
            result.push(col);
        }

        if result.is_empty() {
            if !pinned.is_empty() {
                result.push(pinned[0]);
            } else {
                result.push(scrollable[start_idx]);
            }
        }

        result
    }

    pub fn snapshot(&self, viewport_rows: usize, table_width: u16) -> ViewSnapshot {
        let headers = self.preview.headers();
        let col_indices = self.visible_table_columns(table_width);

        let mut visible_rows = Vec::new();
        let mut visible_row_indices = Vec::new();
        for &row_idx in &self.visible_table_rows(viewport_rows) {
            if let Some(fields) = self.preview.row_fields(row_idx) {
                visible_row_indices.push(row_idx);
                visible_rows.push(fields);
            }
        }

        let visible_columns = col_indices
            .iter()
            .filter_map(|&col_idx| {
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
                    pinned: self.is_column_pinned(col_idx),
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
    fn row_value_filter_narrows_visible_rows() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        let total = model.preview.row_count();
        model
            .set_column_value_filter(0, ">0".to_string())
            .expect("numeric filter");
        let matching = model.matching_row_indices();
        assert!(matching.len() < total);
        assert!(model.column_has_value_filter(0));
    }

    #[test]
    fn toggle_column_borders_flips_session_state() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        let initial = model.view.show_column_borders;
        model.toggle_column_borders();
        assert_ne!(model.view.show_column_borders, initial);
        assert_eq!(model.column_separator_width(), 1);
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

    #[test]
    fn hide_selected_columns_removes_from_table() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.multi_selected_cols = vec![0, 1];
        model.hide_selected_columns().expect("hide");
        assert!(model.is_column_hidden(0));
        assert!(model.is_column_hidden(1));
        assert!(!model.table_visible_columns().contains(&0));
        assert_eq!(model.table_visible_columns().len(), 98);
    }

    #[test]
    fn cannot_hide_every_column() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.multi_selected_cols = (0..100).collect();
        assert!(model.hide_selected_columns().is_err());
    }

    #[test]
    fn ctrl_click_multi_select_adds_columns() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.select_column_click(0, false, 10);
        assert!(model.view.multi_selected_cols.is_empty());
        model.select_column_click(1, true, 10);
        assert_eq!(model.view.multi_selected_cols, vec![0, 1]);
        model.select_column_click(2, true, 10);
        assert_eq!(model.view.multi_selected_cols, vec![0, 1, 2]);
        model.select_column_click(1, true, 10);
        assert_eq!(model.view.multi_selected_cols, vec![0, 1, 2]);
    }

    #[test]
    fn hide_selected_rows_removes_from_table() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.multi_selected_rows = vec![0, 1, 2];
        model.hide_selected_rows().expect("hide rows");
        assert!(model.is_row_hidden(0));
        assert!(model.is_row_hidden(2));
        let matching = model.matching_row_indices();
        assert!(!matching.contains(&0));
        assert!(!matching.contains(&2));
    }

    #[test]
    fn cannot_hide_every_row() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        let total = model.preview.row_count();
        model.view.multi_selected_rows = (0..total).collect();
        assert!(model.hide_selected_rows().is_err());
    }

    #[test]
    fn ctrl_click_row_multi_select_builds_range() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.select_table_cell_click(0, 0, false, 10);
        assert!(!model.cell_range_active());
        model.select_table_cell_click(1, 0, true, 10);
        assert!(model.is_cell_in_selection_range(1, 0));
        model.select_table_cell_click(3, 2, true, 10);
        for row in 1..=3 {
            for col in 0..=2 {
                assert!(model.is_cell_in_selection_range(row, col));
            }
        }
        assert!(!model.is_cell_in_selection_range(0, 0));
        assert_eq!(model.rows_for_bulk_action(), (1..=3).collect::<Vec<_>>());
    }

    #[test]
    fn ctrl_drag_cell_range_selects_rectangle() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.set_cell_range_corners(2, 1, 5, 3);
        assert!(model.is_cell_in_selection_range(2, 1));
        assert!(model.is_cell_in_selection_range(5, 3));
        assert!(model.is_cell_in_selection_range(4, 2));
        assert!(!model.is_cell_in_selection_range(1, 1));
        assert!(!model.is_cell_in_selection_range(2, 4));
        assert_eq!(model.rows_for_bulk_action(), vec![2, 3, 4, 5]);
    }

    #[test]
    fn space_toggle_follows_last_axis() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.set_multi_select_axis(MultiSelectAxis::Column);
        model.view.selected_col = 2;
        model.toggle_multi_select_at_focus();
        assert_eq!(model.view.multi_selected_cols, vec![2]);

        model.set_multi_select_axis(MultiSelectAxis::Row);
        model.view.selected_row = 5;
        model.toggle_multi_select_at_focus();
        assert_eq!(model.view.multi_selected_rows, vec![5]);
        assert!(model.view.multi_selected_cols.is_empty());
    }

    #[test]
    fn row_and_column_multi_select_are_mutually_exclusive() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.multi_selected_cols = vec![0, 1];
        model.set_multi_select_axis(MultiSelectAxis::Row);
        assert_eq!(model.view.multi_selected_cols, vec![0, 1]);

        model.view.selected_row = 5;
        model.toggle_multi_select_at_focus();
        assert_eq!(model.view.multi_selected_rows, vec![5]);
        assert!(model.view.multi_selected_cols.is_empty());

        model.view.multi_selected_rows = vec![2, 3];
        model.set_multi_select_axis(MultiSelectAxis::Column);
        assert_eq!(model.view.multi_selected_rows, vec![2, 3]);

        model.view.selected_col = 4;
        model.toggle_multi_select_at_focus();
        assert_eq!(model.view.multi_selected_cols, vec![4]);
        assert!(model.view.multi_selected_rows.is_empty());

        model.view.multi_selected_cols = vec![6];
        model.select_table_cell_click(10, 5, true, 10);
        assert!(model.is_cell_in_selection_range(10, 5));
        assert!(model.view.multi_selected_cols.is_empty());

        model.set_cell_range_corners(11, 0, 12, 0);
        model.select_column_click(7, true, 10);
        assert_eq!(model.view.multi_selected_cols, vec![7]);
        assert!(model.view.multi_selected_rows.is_empty());
    }

    #[test]
    fn hide_from_command_respects_sidebar_focus() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.column_sidebar_focused = true;
        model.view.multi_selected_cols = vec![0];
        model.hide_from_command().expect("hide column");
        assert!(model.is_column_hidden(0));

        model.view.column_sidebar_focused = false;
        model.view.multi_selected_rows = vec![1];
        model.hide_from_command().expect("hide row");
        assert!(model.is_row_hidden(1));
    }

    #[test]
    fn hide_from_command_hides_columns_when_table_has_column_axis() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.column_sidebar_focused = false;
        model.set_multi_select_axis(MultiSelectAxis::Column);
        model.view.multi_selected_cols = vec![2];
        model.hide_from_command().expect("hide column from table");
        assert!(model.is_column_hidden(2));
        assert!(!model.is_row_hidden(2));
    }

    #[test]
    fn unhide_selected_columns_restores_table() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.multi_selected_cols = vec![0, 1];
        model.hide_selected_columns().expect("hide");
        model.unhide_selected_columns().expect("unhide");
        assert!(!model.is_column_hidden(0));
        assert!(!model.is_column_hidden(1));
        assert_eq!(model.table_visible_columns().len(), 100);
    }

    #[test]
    fn unhide_selected_rows_restores_table() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.multi_selected_rows = vec![0, 1];
        model.hide_selected_rows().expect("hide");
        model.unhide_selected_rows().expect("unhide");
        assert!(!model.is_row_hidden(0));
        assert!(!model.is_row_hidden(1));
        let matching = model.matching_row_indices();
        assert!(matching.contains(&0));
        assert!(matching.contains(&1));
    }

    #[test]
    fn unhide_all_rows_when_selection_not_hidden() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.multi_selected_rows = vec![5];
        model.hide_selected_rows().expect("hide");
        model.view.selected_row = 10;
        model.unhide_selected_rows().expect("unhide all");
        assert!(!model.is_row_hidden(5));
    }

    #[test]
    fn move_selected_column_skips_hidden() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.selected_col = 0;
        model.view.column_hidden[1] = true;
        model.move_selected_column(1, 20);
        assert_eq!(model.view.selected_col, 2);
        model.move_selected_column(-1, 20);
        assert_eq!(model.view.selected_col, 0);
    }

    #[test]
    fn move_selected_sidebar_column_changes_selection() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.selected_col = 0;
        model.move_selected_sidebar_column(1, 20);
        assert_eq!(model.view.selected_col, 1);
        model.move_selected_sidebar_column(-1, 20);
        assert_eq!(model.view.selected_col, 0);
    }

    #[test]
    fn pin_selected_columns_keeps_them_in_viewport() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.selected_col = 5;
        model.toggle_pin_selected_columns();
        assert!(model.is_column_pinned(5));
        model.view.col_offset = 50;
        let visible = model.visible_table_columns(40);
        assert!(visible.contains(&5));
        assert_eq!(visible.first().copied(), Some(5));
    }

    #[test]
    fn pinned_columns_stay_left_when_scrolling() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.selected_col = 0;
        model.toggle_pin_selected_columns();
        model.view.selected_col = 10;
        model.toggle_pin_selected_columns();
        model.view.col_offset = 20;
        let visible = model.visible_table_columns(60);
        assert_eq!(visible.first().copied(), Some(0));
        assert!(visible.contains(&10));
    }

    #[test]
    fn toggle_pin_selected_columns_bulk() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.multi_selected_cols = vec![1, 3];
        model.toggle_pin_selected_columns();
        assert!(model.is_column_pinned(1));
        assert!(model.is_column_pinned(3));
        model.toggle_pin_selected_columns();
        assert!(!model.is_column_pinned(1));
        assert!(!model.is_column_pinned(3));
    }

    #[test]
    fn pinned_columns_list_first_in_sidebar() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.selected_col = 50;
        model.toggle_pin_selected_columns();
        model.view.selected_col = 10;
        model.toggle_pin_selected_columns();
        let sidebar = model.filtered_sidebar_columns();
        assert_eq!(sidebar.first().copied(), Some(50));
        assert_eq!(sidebar.get(1).copied(), Some(10));
        assert_eq!(model.pinned_table_columns(), vec![50, 10]);
        assert!(!model.is_column_pinned(sidebar[2]));
    }

    #[test]
    fn unpinning_preserves_order_of_remaining_pinned_columns() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        for col in [50, 10, 30] {
            model.view.selected_col = col;
            model.toggle_pin_selected_columns();
        }
        model.view.selected_col = 10;
        model.toggle_pin_selected_columns();
        assert_eq!(model.pinned_table_columns(), vec![50, 30]);
    }

    #[test]
    fn hidden_columns_appear_at_end_of_sidebar() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.selected_col = 0;
        model.toggle_pin_selected_columns();
        model.view.selected_col = 5;
        model.hide_selected_columns().expect("hide");
        let sidebar = model.filtered_sidebar_columns();
        let hidden_pos = sidebar.iter().position(|&c| c == 5).expect("hidden col listed");
        let last_visible_pos = sidebar
            .iter()
            .rposition(|&c| !model.is_column_hidden(c))
            .expect("visible cols");
        assert!(hidden_pos > last_visible_pos);
        assert_eq!(sidebar.first().copied(), Some(0));
    }

    #[test]
    fn pin_selected_rows_keeps_them_in_viewport() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.selected_row = 50;
        model.toggle_pin_selected_rows();
        assert!(model.is_row_pinned(50));
        model.view.row_offset = 40;
        model.matching_row_indices();
        let visible = model.visible_table_rows(10);
        assert!(visible.contains(&50));
        assert_eq!(visible.first().copied(), Some(50));
    }

    #[test]
    fn pinned_rows_use_chronological_order() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.selected_row = 50;
        model.toggle_pin_selected_rows();
        model.view.selected_row = 10;
        model.toggle_pin_selected_rows();
        model.matching_row_indices();
        let matching = model.cached_matching_rows().unwrap();
        assert_eq!(model.pinned_table_rows(matching), vec![50, 10]);
    }

    #[test]
    fn toggle_pin_selected_rows_bulk() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.multi_selected_rows = vec![1, 3];
        model.toggle_pin_selected_rows();
        assert!(model.is_row_pinned(1));
        assert!(model.is_row_pinned(3));
        model.toggle_pin_selected_rows();
        assert!(!model.is_row_pinned(1));
        assert!(!model.is_row_pinned(3));
    }

    #[test]
    fn context_menu_action_preserves_column_multi_select() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.multi_selected_cols = vec![1, 3];
        model.view.selected_col = 1;
        model.focus_column_for_context_action(5, 20, false);
        assert_eq!(model.view.multi_selected_cols, vec![1, 3, 5]);
        assert_eq!(model.view.selected_col, 5);
        model.focus_column_for_context_action(3, 20, false);
        assert_eq!(model.view.multi_selected_cols, vec![1, 3]);
        model.focus_column_for_context_action(2, 20, true);
        assert!(model.view.multi_selected_cols.is_empty());
        assert_eq!(model.view.selected_col, 2);
    }

    #[test]
    fn row_navigation_order_pins_first() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.selected_row = 50;
        model.toggle_pin_selected_rows();
        model.view.selected_row = 10;
        model.toggle_pin_selected_rows();
        model.matching_row_indices();
        let matching = model.cached_matching_rows().unwrap();
        let order = model.table_row_navigation_order(matching);
        assert_eq!(order.first().copied(), Some(50));
        assert_eq!(order.get(1).copied(), Some(10));
        let first_scrollable = order
            .iter()
            .find(|&&row| !model.is_row_pinned(row))
            .copied()
            .expect("scrollable row");
        model.view.selected_row = 10;
        model.move_selected_row(1);
        assert_eq!(model.view.selected_row, first_scrollable);
        model.view.selected_row = first_scrollable;
        model.move_selected_row(-1);
        assert_eq!(model.view.selected_row, 10);
    }

    #[test]
    fn add_column_to_selection_builds_multi_select() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.selected_col = 1;
        assert!(model.view.multi_selected_cols.is_empty());

        model.add_column_to_selection(3, 20);
        assert_eq!(model.view.multi_selected_cols, vec![1, 3]);
        assert_eq!(model.view.selected_col, 3);

        model.add_column_to_selection(5, 20);
        assert_eq!(model.view.multi_selected_cols, vec![1, 3, 5]);
        assert_eq!(model.view.selected_col, 5);

        model.add_column_to_selection(3, 20);
        assert_eq!(model.view.multi_selected_cols, vec![1, 3, 5]);
    }

    #[test]
    fn add_column_to_selection_on_current_column_stays_single() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        assert_eq!(model.view.selected_col, 0);
        assert!(model.view.multi_selected_cols.is_empty());

        model.add_column_to_selection(0, 20);
        assert!(model.view.multi_selected_cols.is_empty());
        assert_eq!(model.view.selected_col, 0);

        model.add_column_to_selection(1, 20);
        assert_eq!(model.view.multi_selected_cols, vec![0, 1]);
    }

    #[test]
    fn add_row_to_selection_builds_multi_select() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.selected_row = 2;
        assert!(model.view.multi_selected_rows.is_empty());

        model.add_row_to_selection(4);
        assert_eq!(model.view.multi_selected_rows, vec![2, 4]);
        assert_eq!(model.view.selected_row, 4);

        model.add_row_to_selection(6);
        assert_eq!(model.view.multi_selected_rows, vec![2, 4, 6]);

        model.add_row_to_selection(4);
        assert_eq!(model.view.multi_selected_rows, vec![2, 4, 6]);
    }

    #[test]
    fn context_menu_action_preserves_row_multi_select() {
        let path = PathBuf::from("test-data/generated/test_1000x100.csv");
        if !path.exists() {
            return;
        }
        let mut model = AppModel::open(Some(path)).expect("open csv");
        model.view.multi_selected_rows = vec![1, 3];
        model.view.selected_row = 1;
        model.focus_row_for_context_action(5, false);
        assert_eq!(model.view.multi_selected_rows, vec![1, 3, 5]);
        assert_eq!(model.view.selected_row, 5);
        model.focus_row_for_context_action(3, false);
        assert_eq!(model.view.multi_selected_rows, vec![1, 3]);
        model.focus_row_for_context_action(2, true);
        assert!(model.view.multi_selected_rows.is_empty());
        assert_eq!(model.view.selected_row, 2);
    }
}
