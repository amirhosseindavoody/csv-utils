# Getting started

## Prerequisites

- [Pixi](https://pixi.sh/latest/)
- [rustup](https://rustup.rs/) for local `cargo` builds (pixi tasks source `$HOME/.cargo/env`)

## Setup

```bash
pixi install
pixi run build
```

## Binaries

| Binary | Path | Purpose |
|--------|------|---------|
| `csv` | `target/release/csv` | CLI + TUI |
| `csv-utils-web` | `target/release/csv-utils-web` | Browser UI server |

With pixi:

```bash
pixi run run -- stats sample.csv
pixi run tui test-data/generated/test_1000x100.csv
pixi run web-tui
```

## Entry points

```
csv stats <file.csv>
csv unique <file.csv> <col1[,col2,...]> [limit]
csv json <file.csv> [limit]
csv filter <file.csv> <expr> [limit]
csv tui [file.csv]

csv-utils-web [file.csv] [--host HOST] [--port PORT]
```

- **`tui`** — full-screen TUI; optional CSV path (usage hint if omitted).
- **`csv-utils-web`** — serves browser UI at `http://127.0.0.1:8080/` by default.

Pixi tasks run from the **repo root**; extra args are forwarded (`pixi run tui file.csv`, `pixi run run -- stats file.csv`).

## Generate test data

```bash
pixi run gen-test-data
```

See [test-data-generation.md](test-data-generation.md) for dataset sizes.

## Next steps

- [User experience overview](user-experience/overview.md)
- [CLI reference](features/cli.md)
- [TUI](features/tui.md) · [Web UI](features/web.md)
- [Build & packaging](development/build.md)
