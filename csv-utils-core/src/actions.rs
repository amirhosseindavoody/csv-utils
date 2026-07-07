use crate::column::{ColumnKind, NumericRepr};
use crate::model::AppModel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewLayout {
    pub viewport_rows: usize,
    pub table_width: u16,
    pub column_list_height: usize,
}

impl Default for ViewLayout {
    fn default() -> Self {
        Self {
            viewport_rows: 24,
            table_width: 110,
            column_list_height: 20,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewAction {
    RowDelta(i32),
    ColDelta(i32),
    ColumnListDelta(i32),
    PageRows(i32),
    SelectCell { row: usize, col: usize },
    SelectColumn(usize),
    OpenColumnInfo,
    CloseColumnInfo,
    ColumnInfoFocusDelta(i32),
    ColumnInfoApply,
    SetColumnKind { col: usize, kind: ColumnKind },
    SetNumericRepr { col: usize, repr: NumericRepr },
    SetColumnDecimalFormat { col: usize, format: String },
    ColumnInfoDecimalChar(char),
    ColumnInfoDecimalBackspace,
    ToggleHelp,
    CloseHelp,
    GoHome,
    GoEnd,
    SetColumnWidth { col: usize, width: u16 },
    SetRowOffset(usize),
    SetColOffset(usize),
    SetColumnListOffset(usize),
    SetColumnInfoScroll { scroll: usize, viewport: u16 },
}

impl AppModel {
    pub fn tick(&mut self, layout: ViewLayout) {
        self.ensure_column_state();
        self.maybe_update_column_layout();
        self.clamp_selection(layout.viewport_rows.max(1), layout.table_width);
        self.clamp_column_list_offset(layout.column_list_height);
    }

    pub fn apply_action(&mut self, action: ViewAction, layout: ViewLayout) {
        match action {
            ViewAction::RowDelta(delta) => {
                self.couple_table_scroll_to_selection();
                if delta < 0 {
                    self.view.selected_row = self
                        .view
                        .selected_row
                        .saturating_sub((-delta) as usize);
                } else {
                    self.view.selected_row = self.view.selected_row.saturating_add(delta as usize);
                }
            }
            ViewAction::ColDelta(delta) => {
                self.move_selected_column(delta, layout.column_list_height);
            }
            ViewAction::ColumnListDelta(delta) => {
                if delta < 0 {
                    self.view.column_list_offset = self
                        .view
                        .column_list_offset
                        .saturating_sub((-delta) as usize);
                } else {
                    self.view.column_list_offset += delta as usize;
                }
            }
            ViewAction::PageRows(delta) => {
                self.couple_table_scroll_to_selection();
                if delta < 0 {
                    self.view.selected_row = self
                        .view
                        .selected_row
                        .saturating_sub((-delta) as usize);
                } else {
                    self.view.selected_row = self.view.selected_row.saturating_add(delta as usize);
                }
            }
            ViewAction::SelectCell { row, col } => {
                self.couple_table_scroll_to_selection();
                self.view.selected_row = row;
                self.view.selected_col = col;
                self.ensure_column_list_shows_selection(layout.column_list_height);
            }
            ViewAction::SelectColumn(col) => {
                self.couple_table_scroll_to_selection();
                self.view.selected_col = col;
                self.ensure_column_list_shows_selection(layout.column_list_height);
            }
            ViewAction::OpenColumnInfo => self.open_column_info_pane(),
            ViewAction::CloseColumnInfo => self.close_column_info_pane(),
            ViewAction::ColumnInfoFocusDelta(delta) => self.column_info_focus_delta(delta),
            ViewAction::ColumnInfoApply => self.column_info_apply_focus(),
            ViewAction::SetColumnKind { col, kind } => self.set_column_kind(col, kind),
            ViewAction::SetNumericRepr { col, repr } => self.set_numeric_repr(col, repr),
            ViewAction::SetColumnDecimalFormat { col, format } => {
                self.set_column_decimal_format(col, format)
            }
            ViewAction::ColumnInfoDecimalChar(ch) => self.column_info_decimal_push_char(ch),
            ViewAction::ColumnInfoDecimalBackspace => self.column_info_decimal_backspace(),
            ViewAction::ToggleHelp => self.view.show_help = true,
            ViewAction::CloseHelp => self.view.show_help = false,
            ViewAction::GoHome => {
                self.couple_table_scroll_to_selection();
                self.view.selected_row = 0;
            }
            ViewAction::GoEnd => {
                self.couple_table_scroll_to_selection();
                self.view.selected_row = self.preview.row_count().saturating_sub(1);
            }
            ViewAction::SetColumnWidth { col, width } => {
                self.set_column_width(col, width);
            }
            ViewAction::SetRowOffset(offset) => {
                self.set_row_offset(offset, layout.viewport_rows);
            }
            ViewAction::SetColOffset(offset) => {
                self.set_col_offset(offset, layout.table_width);
            }
            ViewAction::SetColumnListOffset(offset) => {
                self.set_column_list_scroll(offset, layout.column_list_height);
            }
            ViewAction::SetColumnInfoScroll { scroll, viewport } => {
                let col = self.view.selected_col;
                let total = self.column_info_content_line_count(col);
                self.set_column_info_scroll_position(scroll, viewport, total);
            }
        }
        self.tick(layout);
    }
}
