# Guiding principles

These principles explain *why* csv-utils is shaped the way it is. For concrete behavior, see the [user guide](index.md#user-guide); for crate layout, see [architecture](architecture.md).

## Purpose

`csv-utils` is a Rust CSV tool with three surfaces:

| Surface | Role |
|---------|------|
| **CLI** | Streaming stats, filters, unique values, and JSON export on large files |
| **TUI** | Interactive table exploration with progressive loading |
| **Web UI** | Same exploration model in a browser via a local HTTP server |

All interactive UIs share one core model (`AppModel`) so behavior stays aligned across terminal and browser.

## Design goals

1. **Fast initial paint** — show headers and the first rows immediately; grow row count in the background.
2. **Simple, predictable CSV parsing** — RFC 4180 via the `csv` crate; preview uses mmap + on-demand record parse.
3. **One shared core** — `csv-utils-core` owns parsing, preview, CLI engine, and view state; frontends are thin.
4. **Progressive disclosure** — CLI for scripts; TUI/web when you need to look around a file.
5. **Honest limits** — document mmap, indexing, and lazy-stats constraints; see [limitations](reference/limitations.md).

## User experience values

- **Keyboard-first in the TUI** — mouse augments (select, scroll, resize), does not replace keys.
- **Parity where it matters** — TUI and web share selection, scrolling, column list, column info panel, and column resize semantics.
- **Terminal-native web fallback** — browser UI follows system light/dark by default; explicit theme override when needed.
- **Fixed-width cells for scanning** — monospace columns auto-fit to header and loaded row content (4–64 chars). Text and dates truncate with middle `...`; numbers rescale (precision/notation) instead of ellipsis.

## Non-goals (for now)

- Full CSV dialect configuration (custom delimiters, etc.).
- Persisted UI state across sessions (column widths, selection, scroll).
- Single in-process cache shared between CLI and TUI invocations.

See [known limitations](reference/limitations.md) for the full list.
