# csv-utils documentation

High-performance CSV utility with a streaming **CLI**, interactive **TUI**, and optional **web UI** — all backed by a shared Rust core library.

**Last verified against:** `2026.6.30+0` (workspace version in `Cargo.toml` / `pixi.toml`; row JSON floating panel via `r`).

## Quick start

```bash
git clone https://github.com/amirhosseindavoody/csv-utils.git
cd csv-utils
pixi install
pixi run build
pixi run csv                          # TUI file picker
pixi run csv -- stats sample.csv      # CLI stats
pixi run csv test-data/generated/test_1000x100.csv
```

Install from another pixi workspace:

```bash
pixi add --git https://github.com/amirhosseindavoody/csv-utils.git --branch main csv-utils
```

See [Getting started](getting-started.md) for prerequisites, entry points, and settings.

## What you get

| Surface | Best for | Entry |
|---------|----------|-------|
| **CLI** | Scripts, pipelines, one-shot queries | `csv stats`, `unique`, `json`, `filter` |
| **TUI** | Exploring files in the terminal | `csv` or `csv <file.csv>` |
| **Web UI** | Same explorer in a browser | `:web` inside the TUI |

Press `?` in the TUI for inline help. Type `:web` to hand off to the browser (the terminal view closes; Ctrl+C stops the server).

## User guide

| Document | Contents |
|----------|----------|
| [Getting started](getting-started.md) | Setup, binaries, entry points, settings |
| [User experience overview](user-experience/overview.md) | Choosing CLI vs TUI vs web |
| [CLI reference](features/cli.md) | Commands, filter syntax, examples |
| [TUI](features/tui.md) | Layout, keyboard, mouse, commands |
| [Web UI](features/web.md) | `:web` handoff, theme, JSON API |

## Reference

| Document | Contents |
|----------|----------|
| [Data loading](reference/data-loading.md) | Preview pipeline, mmap, threading |
| [CSV parsing & display](reference/csv-parsing.md) | `split_row`, column types, cell formatting |
| [Known limitations](reference/limitations.md) | Intentional trade-offs and constraints |

## Design notes

| Document | Contents |
|----------|----------|
| [Large-file preview](design/large-file-preview.md) | mmap + offset index architecture |
| [Row filtering](design/row-filtering.md) | Filter expressions and cache strategy |
| [Settings config](design/settings-config.md) | Global + local `csv-utils.json` |
| [Performance & TUI responsiveness](design/performance-tui-responsiveness.md) | Prioritized proposals for scan/filter/sort/draw |

## Developer docs

| Document | Contents |
|----------|----------|
| [Principles](principles.md) | Goals, non-goals, UX values |
| [Architecture](architecture.md) | Crates, modules, shared view model |
| [Build & packaging](development/build.md) | Pixi tasks, conda recipe, CI |
| [Test data generation](test-data-generation.md) | Synthetic CSV datasets for development |

## Repository

- **Source:** [github.com/amirhosseindavoody/csv-utils](https://github.com/amirhosseindavoody/csv-utils)
- **Docs site:** [amirhosseindavoody.github.io/csv-utils](https://amirhosseindavoody.github.io/csv-utils/)
- **License:** MIT
