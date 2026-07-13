# User experience overview

csv-utils offers three ways to work with CSV files. They share parsing rules and differ mainly in how you interact.

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
| **CLI** | Pipelines, automation, one-shot queries | Commands + stdout |
| **TUI** | Exploring unknown files in the terminal | Table + column sidebar; keys and mouse |
| **Web UI** | Same exploration in a browser | Local page + JSON API |

## Shared exploration (TUI + web)

Both interactive UIs use the same `AppModel`:

- Progressive loading (first rows immediately, background scan for the rest)
- Row and column selection with scroll windows
- Column sidebar with its own scroll offset
- Column info panel (types, formatting, stats, per-column row filter)
- Pin / hide columns and rows; multi-select and cell ranges
- Session sort on one column
- Per-column width resize (auto-fit within 4–64 characters)
- Help overlay (`?`)
- Row-as-JSON panel (`r`)

| Aspect | TUI | Web |
|--------|-----|-----|
| Rendering | ratatui + crossterm | HTML + fetch API |
| Theme | Terminal colors | System light/dark + manual toggle |
| Leave | `q` (panel → picker → quit) | Close tab; Ctrl+C stops the server |
| Layout size | Follows terminal resize | Fixed from the terminal size at `:web` handoff |

## CLI

CLI commands stream the file and parse every row. They do not use the preview buffer or `AppModel`. Prefer the CLI for aggregates or filtered JSON; prefer TUI/web for cell-level browsing.

## Choosing a surface

| Goal | Use |
|------|-----|
| Counts, nulls, max width per column | `stats` |
| Distinct combinations | `unique` |
| Filter rows to JSON | `filter` |
| Sample rows as JSON | `json` |
| Scan a wide file visually | TUI (`csv [file]`) |
| Continue the same session in a browser | `:web` in the TUI |

## Detailed guides

- [CLI](features/cli.md)
- [TUI](features/tui.md)
- [Web UI](features/web.md)
