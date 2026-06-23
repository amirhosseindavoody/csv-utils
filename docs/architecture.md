# Architecture

Runtime structure of csv-utils: binaries, shared core, and the view model used by TUI and web frontends.

## System overview

```mermaid
flowchart TB
  bin[csv-utils binary] --> cli[cli.rs]
  bin --> tui[tui/app.rs]
  web[csv-utils-web binary] --> web_srv[web/server.rs]
  cli --> engine[csv-utils-core/engine.rs]
  tui --> model[csv-utils-core/model.rs]
  web_srv --> model
  model --> preview[csv-utils-core/preview.rs]
  model --> schema[csv-utils-core/schema.rs]
  engine --> schema
  tui --> ratatui[ratatui + crossterm]
  web_srv --> axum[axum + tokio]
```

| Layer | Role |
|--------|------|
| `csv-utils/src/main.rs` | Clap CLI dispatch; `tui` subcommand |
| `csv-utils/src/cli.rs` | CLI command runners |
| `csv-utils/src/tui/app.rs` | ratatui renderer, event loop, input |
| `csv-utils-web/src/main.rs` | Browser UI server entry |
| `csv-utils-web/src/server.rs` | axum routes, JSON API, embedded HTML |
| `csv-utils-core/` | Parsing, preview, CLI engine, `AppModel`, actions, client view |

## Workspace layout

```
Cargo.toml                   # workspace root
csv-utils-core/              # shared library
csv-utils/                   # CLI + TUI binary
csv-utils-web/               # browser server binary
recipe/recipe.yaml           # conda package (rattler-build)
scripts/                     # test data + TUI capture
test-data/generated/
docs/
```

## Shared view model

Interactive UIs do not duplicate CSV state. They use:

| Type | Location | Role |
|------|----------|------|
| `AppModel` | `model.rs` | File path, `PreviewData`, `TableViewState`, scan thread |
| `TableViewState` | `model.rs` | Selection, scroll offsets, column widths, UI flags |
| `ViewAction` | `actions.rs` | Keyboard/mouse-style mutations (row/col delta, resize, etc.) |
| `ViewLayout` | `actions.rs` | Viewport dimensions for clamping (rows, table width, sidebar height) |
| `ClientView` | `client_view.rs` | JSON snapshot for browser clients |
| `ViewSnapshot` | `model.rs` | Richer in-memory snapshot (future/alternate clients) |

Flow:

1. **Input** → TUI event loop or web `POST /api/action` parses intent.
2. **`apply_action`** → mutates `TableViewState`, runs `tick()` (clamp selection/scroll).
3. **Render** → TUI draws from `AppModel`; web returns `ClientView` JSON.

## CLI vs interactive loading

| Path | Reads file | Parses rows |
|------|------------|-------------|
| CLI (`engine.rs`) | Sequential stream | Every data line via `split_row` |
| TUI / web (`preview.rs`) | Header + 128 lines sync, then background append | `split_row` only on visible rows |

CLI commands re-open files; there is no shared cache with TUI/web sessions.

## Module map

```
csv-utils-core/src/
  lib.rs
  schema.rs          # split_row
  predicate.rs       # filter expressions
  preview.rs         # PreviewData, background scan
  stats.rs
  unique.rs
  json_view.rs
  engine.rs          # CLI orchestration
  column.rs          # header-prefix type heuristics
  model.rs           # AppModel, TableViewState, format_cell
  actions.rs         # ViewAction, apply_action
  client_view.rs     # ClientView JSON

csv-utils/src/
  main.rs
  cli.rs
  tui/app.rs

csv-utils-web/src/
  main.rs
  server.rs
  assets.rs
  index.html
```

## Related docs

- [Data loading](reference/data-loading.md) — preview APIs and threading
- [CSV parsing](reference/csv-parsing.md) — `split_row` and display rules
- [Build & packaging](development/build.md) — pixi tasks and conda recipe
