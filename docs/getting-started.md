# Getting started

## Prerequisites

- [Pixi](https://pixi.sh/latest/)
- Optional: [rustup](https://rustup.rs/) if you run bare `cargo` outside Pixi (Pixi tasks source `$HOME/.cargo/env` when present)

## Setup

```bash
pixi install
pixi run build
```

## Binary

| Binary | Path | Purpose |
|--------|------|---------|
| `csv` | `target/release/csv` | CLI + TUI (+ browser UI via `:web`) |

```bash
pixi run csv -- stats sample.csv
pixi run csv test-data/generated/test_1000x100.csv
```

## Entry points

```
csv [subcommand]
csv [file.csv]              # TUI (file picker if omitted)
csv stats <file.csv>
csv unique <file.csv> <col1[,col2,...]> [limit]
csv json <file.csv> [limit]
csv filter <file.csv> <expr> [limit]
csv tui [file.csv]          # alias for csv [file.csv]
```

- **`csv`** / **`csv <file>`** — TUI (picker when no path is given)
- **`csv tui`** — same as above
- **`:web`** (inside the TUI) — open the browser UI on a free local port and exit the terminal view (Ctrl+C stops the server)

Pixi tasks run from the **repo root**. Extra args are forwarded (`pixi run csv file.csv`, `pixi run csv -- stats file.csv`).

## Settings

Defaults live in `~/.config/csv-utils/csv-utils.json` (created on first TUI or web launch). Optional project overrides go in `./csv-utils.json` in the working directory; local fields override global ones. See [settings config](design/settings-config.md).

## Test data

```bash
pixi run gen-test-data
```

See [test-data-generation.md](test-data-generation.md) for dataset sizes.

## Next steps

- [User experience overview](user-experience/overview.md)
- [CLI reference](features/cli.md)
- [TUI](features/tui.md) · [Web UI](features/web.md)
- [Build & packaging](development/build.md)
