# CLI

Streaming commands for large CSV files.

Running `csv` with no arguments opens the [TUI](tui.md) file picker. `csv <file.csv>` opens that file in the TUI (`csv tui <file.csv>` is the same).

Global flags: `-h` / `--help`, `-V` / `--version`.

## Commands

| Command | Usage | Default limit | Behavior |
|---------|--------|---------------|----------|
| `stats` | `stats <file.csv>` | — | Per-column row / null / non-null counts and `max_width` |
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

- Operators: `=`, `!=`, `>`, `<`, `contains`, `in`
- Comma-separated conditions are **AND**ed
- Examples: `city=Tehran`, `age>30`, `name contains Ali`, `city in Tehran|Paris`

## Loading model

CLI commands read the file sequentially and parse every data line. TUI/web preview mmap the file, index records in the background, and parse visible rows on demand.

See [data loading](../reference/data-loading.md) for the interactive path.
