# CSV parsing & display

## Row parsing

`schema::split_row` — quoted fields, `""` escape, comma split outside quotes.

Location: `csv-utils-core/src/schema.rs`

CLI, preview rendering, and tests all use this function.

## Cell display

Formatting lives in `csv-utils-core/src/display.rs`:

- Fixed-width monospace cells
- Non-printable bytes → `.`
- **Text / date** — middle ellipsis (`...`) when wider than the column
- **Int / float** — rescaled to fit (reduced precision, general or scientific notation); no ellipsis
- Column widths **auto-fit** to header + loaded row content, clamped **4–64** (`MIN_COLUMN_WIDTH` / `MAX_COLUMN_WIDTH` in `model.rs`)
- Manual column resize (TUI/web drag) locks that column until a new file is opened

Entry point: `format_cell_for_column(text, width, kind, repr)`.

## Column types (display only)

Inferred from **loaded cell values** when kind is `Auto`:

| Inferred kind | Rule | Alignment |
|---------------|------|-----------|
| `date` | All non-empty values match `YYYY-MM-DD` | left |
| `int` | All non-empty values parse as integers | right |
| `float` | All non-empty values parse as floats | right |
| `text` | Otherwise | left |

Location: `csv-utils-core/src/column.rs` (`infer_column_kind_from_values`).

Override the selected column with **`t`**, which opens a format picker for **auto**, **text**, **date**, **int**, and **float**. For numeric columns, the picker also offers **general** vs **scientific** representation (affects formatting and auto-fit width).

The column sidebar always shows the stored type (and inferred type when set to auto).

Types affect alignment, sidebar labels, truncation vs rescaling, and numeric notation only; they do not change CLI parsing.
