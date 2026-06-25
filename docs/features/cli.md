# CLI

Streaming commands for large CSV files. Implementation: `csv-utils-core/src/engine.rs`.

## Commands

| Command | Usage | Default limit | Behavior |
|---------|--------|---------------|----------|
| `stats` | `stats <file.csv>` | — | Per-column row/null/non-null counts and `max_width` |
| `unique` | `unique <file> <col1[,col2,...]> [limit]` | 50 | Distinct value combinations as JSON objects |
| `json` | `json <file> [limit]` | 20 | Rows as JSON objects |
| `filter` | `filter <file> <expr> [limit]` | 50 | Matching rows as JSON objects |

## Examples

```bash
pixi run run -- stats sample.csv
pixi run run -- unique sample.csv city,active 100
pixi run run -- filter sample.csv city=Tehran,active=true 25
pixi run run -- filter sample.csv age>30 50
pixi run run -- filter sample.csv "name contains Ali" 20
pixi run run -- filter sample.csv "city in Tehran|Paris" 20
pixi run run -- json sample.csv 10
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
