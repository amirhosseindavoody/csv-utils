# User experience overview

csv-utils offers three ways to work with CSV files. They share parsing rules but differ in interaction model.

## Surfaces

```
                    ┌─────────────────┐
                    │  csv-utils-core │
                    │  schema, preview│
                    │  AppModel       │
                    └────────┬────────┘
           ┌─────────────────┼─────────────────┐
           ▼                 ▼                 ▼
      ┌─────────┐      ┌──────────┐     ┌──────────┐
      │   CLI   │      │   TUI    │     │  Web UI  │
      │ scripts │      │ terminal │     │ browser  │
      └─────────┘      └──────────┘     └──────────┘
```

| Surface | Best for | Interaction |
|---------|----------|-------------|
| **CLI** | Pipelines, automation, one-shot queries | Commands + stdout JSON/text |
| **TUI** | Exploring unknown files in the terminal | Table + column sidebar, keys + mouse |
| **Web UI** | Same exploration without a terminal UI | HTTP page + JSON API |

## Shared exploration behavior (TUI + web)

Both interactive UIs use the same `AppModel`:

- Progressive row loading (128 lines first, background scan)
- Row/column selection with scroll windows
- Column sidebar with independent list scroll
- Optional column type labels (header-prefix heuristics)
- Per-column width resize (4–64 characters, default 18)
- Help overlay (`?`)

Differences:

| Aspect | TUI | Web |
|--------|-----|-----|
| Rendering | ratatui + crossterm | HTML table + fetch API |
| Theme | Terminal colors | System light/dark + manual toggle |
| Quit | `q` | Close tab / stop server |
| Layout size | Terminal resize | Fixed layout constants in server |

## CLI behavior

CLI commands stream the full file and parse every row. They do not use the preview buffer or `AppModel`. Use CLI when you need aggregated output or filtered JSON, not cell-by-cell browsing.

## Choosing a surface

| Goal | Use |
|------|-----|
| Count nulls, max width per column | `stats` |
| Distinct combinations | `unique` |
| Filter rows to JSON | `filter` |
| Sample rows as JSON | `json` |
| Scan a wide file visually | `tui` or `web` |
| Share view on LAN | `csv-utils-web --host 0.0.0.0` |

## Detailed specs

- [CLI](features/cli.md)
- [TUI](features/tui.md)
- [Web UI](features/web.md)
