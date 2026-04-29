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

## Status

- Project scaffolded.
- CLI command surface implemented with a streaming baseline.
- TUI mode bootstrapped (ncurses integration is the next step).
