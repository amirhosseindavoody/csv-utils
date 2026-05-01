# csv-utils

High-performance CSV utility in Zig with CLI and TUI modes.

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

## Status

- Project scaffolded.
- CLI command surface implemented with a streaming baseline.
- TUI mode bootstrapped (ncurses integration is the next step).
