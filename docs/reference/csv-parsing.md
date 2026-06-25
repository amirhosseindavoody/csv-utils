# CSV parsing & display

## Row parsing

Field splitting uses the Rust [`csv`](https://docs.rs/csv) crate (RFC 4180).

| Path | Entry point | Notes |
|---|---|---|
| TUI / web preview | `PreviewData::row_fields` | Parses a mmap slice for one indexed record |
| CLI | `schema::split_row` | One record per line; wraps `csv` on the line bytes |
| Tests / helpers | `schema::read_fields_from_slice` | Parse any single-record byte slice |

Location: `csv-utils-core/src/schema.rs`, `csv-utils-core/src/preview.rs`

Quoted fields, `""` escape, commas inside quotes, and embedded newlines in quoted
fields are supported on the preview path.

## Cell display

Formatting lives in `csv-utils-core/src/display.rs`:

- Fixed-width monospace cells
- Non-printable bytes → `.`
- **Text / date** — middle ellipsis (`...`) when wider than the column
- **Int / float** — rescaled to fit (reduced precision, general or scientific notation); no ellipsis
- Column widths **auto-fit** to header + indexed row content, clamped **4–64** (`MIN_COLUMN_WIDTH` / `MAX_COLUMN_WIDTH` in `model.rs`)
- Manual column resize (TUI/web drag) locks that column until a new file is opened

Entry point: `format_cell_for_column(text, width, kind, repr)`.

## Column types (display only)

Inferred from **indexed cell values** when kind is `Auto`:

| Inferred kind | Rule | Alignment |
|---------------|------|-----------|
| `date` | All non-empty values match `YYYY-MM-DD` | left |
| `int` | All non-empty values parse as integers | right |
| `float` | All non-empty values parse as floats | right |
| `text` | Otherwise | left |

Location: `csv-utils-core/src/column.rs` (`infer_column_kind_from_values`).

Press **`c`** to open the column info panel: change type (options depend on inferred data) and representation, and view type-specific statistics (lazy; computed from indexed rows while the panel is open).

Types affect alignment, sidebar labels, truncation vs rescaling, and numeric notation only; they do not change CLI parsing.
