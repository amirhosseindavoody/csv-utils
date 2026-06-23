use crate::column::is_right_aligned;
use crate::display::{format_cell_for_column, truncate_middle};
use crate::model::AppModel;
use crate::schema;
use crate::ViewLayout;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ClientView {
    pub file: String,
    pub row_count: usize,
    pub scan_done: bool,
    pub scan_error: bool,
    pub selected_row: usize,
    pub selected_col: usize,
    pub show_column_types: bool,
    pub show_help: bool,
    pub status_line: String,
    pub column_list_offset: usize,
    pub column_count: usize,
    pub table: ClientTable,
    pub sidebar: Vec<ClientSidebarItem>,
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
        let col_range = self.visible_column_range(layout.table_width);
        let sidebar_start = self.view.column_list_offset;
        let sidebar_end = (sidebar_start + layout.column_list_height).min(headers.len());

        let mut table_rows = Vec::new();
        let mut row_start = 0usize;
        let mut row_end = 0usize;

        for i in 0..layout.viewport_rows {
            let row_idx = self.view.row_offset + i;
            let Some(line) = self.preview.row_line(row_idx) else {
                break;
            };
            if i == 0 {
                row_start = row_idx;
            }
            row_end = row_idx + 1;
            let fields = schema::split_row(&line);
            let row_selected = row_idx == self.view.selected_row;
            let cells = col_range
                .clone()
                .map(|col_idx| {
                    let text = fields.get(col_idx).map(String::as_str).unwrap_or("");
                    let kind = self.effective_column_kind(col_idx);
                    let repr = self.numeric_repr(col_idx);
                    ClientCell {
                        text: format_cell_for_column(
                            text,
                            self.column_width_chars(col_idx),
                            kind,
                            repr,
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

        let columns = col_range
            .clone()
            .map(|col_idx| ClientColumnHeader {
                index: col_idx,
                name: self.format_column_header(col_idx, &headers[col_idx]),
                width: self.column_width_chars(col_idx) as u16,
                selected: col_idx == self.view.selected_col,
            })
            .collect();

        let sidebar = (sidebar_start..sidebar_end)
            .filter_map(|col_idx| {
                let name = headers.get(col_idx)?;
                let stored = self
                    .view
                    .column_kinds
                    .get(col_idx)
                    .copied()
                    .unwrap_or(crate::column::ColumnKind::Auto);
                let effective = self.effective_column_kind(col_idx);
                let label = if self.view.show_column_types {
                    if stored == crate::column::ColumnKind::Auto {
                        format!("{col_idx}: {name} [{}]", effective.label())
                    } else {
                        format!(
                            "{col_idx}: {name} [{}={}]",
                            stored.label(),
                            effective.label()
                        )
                    }
                } else {
                    format!("{col_idx}: {name}")
                };
                let display = truncate_middle(&label, 32);
                Some(ClientSidebarItem {
                    index: col_idx,
                    label: display,
                    selected: col_idx == self.view.selected_col,
                })
            })
            .collect();

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

        ClientView {
            file: self.file_label().to_string(),
            row_count: self.preview.row_count(),
            scan_done: self.preview.scan_done(),
            scan_error: self.preview.scan_error(),
            selected_row: self.view.selected_row,
            selected_col: self.view.selected_col,
            show_column_types: self.view.show_column_types,
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
        }
    }
}
