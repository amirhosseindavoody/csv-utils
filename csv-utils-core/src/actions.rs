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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewAction {
    RowDelta(i32),
    ColDelta(i32),
    ColumnListDelta(i32),
    PageRows(i32),
    SelectCell { row: usize, col: usize },
    SelectColumn(usize),
    OpenColumnFormat,
    CloseColumnFormat,
    ColumnFormatFocusDelta(i32),
    ColumnFormatApply,
    SetColumnKind { col: usize, kind: ColumnKind },
    SetNumericRepr { col: usize, repr: NumericRepr },
    ToggleHelp,
    CloseHelp,
    GoHome,
    GoEnd,
    SetColumnWidth { col: usize, width: u16 },
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
                if delta < 0 {
                    self.view.selected_col = self
                        .view
                        .selected_col
                        .saturating_sub((-delta) as usize);
                } else {
                    self.view.selected_col = self.view.selected_col.saturating_add(delta as usize);
                }
                self.ensure_column_list_shows_selection(layout.column_list_height);
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
                self.view.selected_row = row;
                self.view.selected_col = col;
                self.ensure_column_list_shows_selection(layout.column_list_height);
            }
            ViewAction::SelectColumn(col) => {
                self.view.selected_col = col;
                self.ensure_column_list_shows_selection(layout.column_list_height);
            }
            ViewAction::OpenColumnFormat => self.open_column_format_pane(),
            ViewAction::CloseColumnFormat => self.close_column_format_pane(),
            ViewAction::ColumnFormatFocusDelta(delta) => self.column_format_focus_delta(delta),
            ViewAction::ColumnFormatApply => self.column_format_apply_focus(),
            ViewAction::SetColumnKind { col, kind } => self.set_column_kind(col, kind),
            ViewAction::SetNumericRepr { col, repr } => self.set_numeric_repr(col, repr),
            ViewAction::ToggleHelp => self.view.show_help = true,
            ViewAction::CloseHelp => self.view.show_help = false,
            ViewAction::GoHome => self.view.selected_row = 0,
            ViewAction::GoEnd => {
                self.view.selected_row = self.preview.row_count().saturating_sub(1);
            }
            ViewAction::SetColumnWidth { col, width } => {
                self.set_column_width(col, width);
            }
        }
        self.tick(layout);
    }
}
