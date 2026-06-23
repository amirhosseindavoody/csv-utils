# CSV parsing & display

## Row parsing

`schema::split_row` — quoted fields, `""` escape, comma split outside quotes.

Location: `csv-utils-core/src/schema.rs`

CLI, preview rendering, and tests all use this function.

## Cell display

`format_cell(text, width, align_right)` in `model.rs`:

- Fixed-width monospace cells
- Non-printable bytes → `.`
- Truncation marker → `~` at last visible character
- Default width **18** (`CELL_DISPLAY_WIDTH`); per-column override 4–64

## Column types (display only)

Inferred from **header name prefixes** (matches test data generator):

| Prefix | Kind | Alignment |
|--------|------|-----------|
| `str_` | string | left |
| `long_str_` | long string | left |
| `float_general_` | float | right |
| `float_scientific_` | float (sci) | right |
| `float_mixed_` | float (mixed) | right |
| `int_` | integer | right |
| `date_` | date | left |
| (other) | unknown | left |

Location: `csv-utils-core/src/column.rs`

Types affect alignment and sidebar labels only; they are not inferred from cell values.

Toggle type labels in TUI/web with `t`.
