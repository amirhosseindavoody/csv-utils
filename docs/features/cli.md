# CLI

Streaming commands for large CSV files. Implementation: `csv-utils-core/src/engine.rs`.

Running `csv` with no arguments launches the [TUI](tui.md) file picker.
`csv <file.csv>` opens the file in the TUI directly (`csv tui <file.csv>` is the same).

Global flags: `-h` / `--help`, `-V` / `--version` (prints package version from `Cargo.toml`).

## Commands

| Command | Usage | Default limit | Behavior |
|---------|--------|---------------|----------|
| `stats` | `stats <file.csv>` | — | Per-column row/null/non-null counts and `max_width` |
| `unique` | `unique <file> <col1[,col2,...]> [limit]` | 50 | Distinct value combinations as JSON objects |
| `json` | `json <file> [limit]` | 20 | Rows as JSON objects |
| `filter` | `filter <file> <expr> [limit]` | 50 | Matching rows as JSON objects |

## Examples

```bash
pixi run csv -- stats sample.csv
pixi run csv -- unique sample.csv city,active 100
pixi run csv -- filter sample.csv city=Tehran,active=true 25
pixi run csv -- filter sample.csv age>30 50
pixi run csv -- filter sample.csv "name contains Ali" 20
pixi run csv -- filter sample.csv "city in Tehran|Paris" 20
pixi run csv -- json sample.csv 10
```

## Filter expressions

Parser: `csv-utils-core/predicate.rs`

- Operators: `=`, `!=`, `>`, `<`, `contains`, `in`
- Comma-separated **AND** between conditions
- Examples: `city=Tehran`, `age>30`, `name contains Ali`, `city in Tehran|Paris`

## Loading model

CLI reads the file sequentially and calls `schema::split_row` on every data line.
TUI/web preview mmap the file, index records in the background, and parse only
visible rows on demand.

See [data loading](../reference/data-loading.md) for the interactive path.
