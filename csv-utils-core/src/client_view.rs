use crate::column::is_right_aligned;
use crate::column_stats::ColumnInfo;
use crate::display::{format_cell_for_column, truncate_middle};
use crate::model::AppModel;
use crate::ViewLayout;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ScrollMeta {
    pub offset: usize,
    pub total: usize,
    pub viewport: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientView {
    pub file: String,
    pub row_count: usize,
    pub scan_done: bool,
    pub scan_error: bool,
    pub selected_row: usize,
    pub selected_col: usize,
    pub show_column_info: bool,
    pub column_info: Option<ColumnInfo>,
    pub show_help: bool,
    pub status_line: String,
    pub column_list_offset: usize,
    pub column_count: usize,
    pub table: ClientTable,
    pub sidebar: Vec<ClientSidebarItem>,
    pub table_rows_scroll: ScrollMeta,
    pub table_cols_scroll: ScrollMeta,
    pub sidebar_scroll: ScrollMeta,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientTable {
    pub row_start: usize,
    pub row_end: usize,
    pub columns: Vec<ClientColumnHeader>,
    pub rows: Vec<ClientTableRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientColumnHeader {
    pub index: usize,
    pub name: String,
    pub width: u16,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientTableRow {
    pub index: usize,
    pub selected: bool,
    pub cells: Vec<ClientCell>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientCell {
    pub text: String,
    pub align_right: bool,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientSidebarItem {
    pub index: usize,
    pub label: String,
    pub selected: bool,
}

impl AppModel {
    pub fn client_view(&self, layout: ViewLayout) -> ClientView {
        let headers = self.preview.headers();
        let table_cols = self.table_visible_columns();
        let col_indices = self.visible_table_columns(layout.table_width);
        let filtered_sidebar = self.filtered_sidebar_columns();
        let sidebar_start = self.view.column_list_offset;
        let sidebar_end = (sidebar_start + layout.column_list_height).min(filtered_sidebar.len());

        let mut table_rows = Vec::new();
        let mut row_start = 0usize;
        let mut row_end = 0usize;

        for i in 0..layout.viewport_rows {
            let row_idx = self.view.row_offset + i;
            let Some(fields) = self.preview.row_fields(row_idx) else {
                break;
            };
            if i == 0 {
                row_start = row_idx;
            }
            row_end = row_idx + 1;
            let row_selected = row_idx == self.view.selected_row;
            let cells = col_indices
                .iter()
                .map(|&col_idx| {
                    let text = fields.get(col_idx).map(String::as_str).unwrap_or("");
                    let kind = self.effective_column_kind(col_idx);
                    let repr = self.numeric_repr(col_idx);
                    ClientCell {
                        text: format_cell_for_column(
                            text,
                            self.column_width_chars(col_idx),
                            kind,
                            repr,
                            self.decimal_places_for_column(col_idx),
                        ),
                        align_right: is_right_aligned(kind),
                        selected: row_selected && col_idx == self.view.selected_col,
                    }
                })
                .collect();
            table_rows.push(ClientTableRow {
                index: row_idx,
                selected: row_selected,
                cells,
            });
        }

        let columns = col_indices
            .iter()
            .map(|&col_idx| ClientColumnHeader {
                index: col_idx,
                name: self.format_column_header(col_idx, &headers[col_idx]),
                width: self.column_width_chars(col_idx) as u16,
                selected: col_idx == self.view.selected_col,
            })
            .collect();

        let sidebar = (sidebar_start..sidebar_end)
            .filter_map(|pos| {
                let col_idx = *filtered_sidebar.get(pos)?;
                let name = headers.get(col_idx)?;
                let label = self.format_sidebar_column_label(col_idx, name);
                let display = truncate_middle(&label, 32);
                Some(ClientSidebarItem {
                    index: col_idx,
                    label: display,
                    selected: col_idx == self.view.selected_col,
                })
            })
            .collect();

        let row_total = self
            .cached_matching_rows()
            .map(|m| m.len())
            .unwrap_or_else(|| self.preview.row_count());

        let status_line = format!(
            "row {}/{}  col {}/{}  {}",
            self.view.selected_row + 1,
            self.preview.row_count().max(1),
            self.view.selected_col + 1,
            headers.len().max(1),
            if self.preview.scan_error() {
                "error"
            } else if self.preview.scan_done() {
                "loaded"
            } else {
                "loading…"
            }
        );

        let column_info = if self.view.show_column_info {
            Some(self.column_info(self.view.selected_col))
        } else {
            None
        };

        ClientView {
            file: self.file_label().to_string(),
            row_count: self.preview.row_count(),
            scan_done: self.preview.scan_done(),
            scan_error: self.preview.scan_error(),
            selected_row: self.view.selected_row,
            selected_col: self.view.selected_col,
            show_column_info: self.view.show_column_info,
            column_info,
            show_help: self.view.show_help,
            status_line,
            column_list_offset: self.view.column_list_offset,
            column_count: headers.len(),
            table: ClientTable {
                row_start: if table_rows.is_empty() { 0 } else { row_start },
                row_end,
                columns,
                rows: table_rows,
            },
            sidebar,
            table_rows_scroll: ScrollMeta {
                offset: self.view.row_offset,
                total: row_total,
                viewport: layout.viewport_rows,
            },
            table_cols_scroll: ScrollMeta {
                offset: self.view.col_offset,
                total: table_cols.len(),
                viewport: col_indices.len(),
            },
            sidebar_scroll: ScrollMeta {
                offset: self.view.column_list_offset,
                total: filtered_sidebar.len(),
                viewport: layout.column_list_height,
            },
        }
    }
}
