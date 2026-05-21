# csv-utils

High-performance CSV utility in Zig with CLI and TUI modes.

Design and behavior (kept in sync with the code): **[docs/DESIGN.md](docs/DESIGN.md)**.

## Prerequisites

- [Pixi](https://pixi.sh/latest/)

## Setup

```bash
pixi install
```

## Run

```bash
pixi run run -- stats sample.csv
pixi run run -- unique sample.csv city,active 100
pixi run run -- filter sample.csv city=Tehran,active=true 25
pixi run run -- filter sample.csv age>30 50
pixi run run -- filter sample.csv "name contains Ali" 20
pixi run run -- filter sample.csv "city in Tehran|Paris" 20
pixi run run -- json sample.csv 10
pixi run tui sample.csv
```

## Testing

### Test TUI mode using Pixi tasks

```bash
# Generate all configured benchmark datasets
pixi run gen-test-data

# Optional: generate only smaller datasets for quick iteration
pixi run gen-test-data --datasets 1000x100 10000x1000
# Run TUI on generated test data
pixi run tui test-data/generated/test_1000x100.csv
```

### Unit tests

```bash
pixi run test
```

The `test` run also includes a **preview-load benchmark** test (prints timing when `test-data/generated/test_1000x100.csv` exists; otherwise skipped).

### CSV preview load benchmark (sync header + N lines)

Measures reading the header (parsed into columns) plus up to `limit` raw data lines in one pass (`loadPreviewLimited`). The TUI loads the header plus an initial screenful of rows before the first paint, then streams the rest on a background thread so the table fills in as you scroll. CLI commands like `stats` scan the whole file and parse every line with `splitRow`, which is heavier.

```bash
# No args: default file test-data/generated/test_1000x100.csv, limit 500 (generate it with gen-test-data first)
pixi run bench-parse

# Explicit path and limit
pixi run bench-parse -- test-data/generated/test_1000x100.csv 500

# Limit only (same default file)
pixi run bench-parse -- 1000

# Also time splitRow on every loaded row (closer to per-cell work when drawing)
pixi run bench-parse -- --parse-fields
```

## Status

- Project scaffolded.
- CLI command surface implemented with a streaming baseline.
- TUI mode bootstrapped (ncurses integration is the next step).
