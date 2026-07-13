# Guiding principles

Why csv-utils is shaped the way it is. For behavior, see the [user guide](index.md#user-guide); for crate layout, see [architecture](architecture.md).

## Purpose

| Surface | Role |
|---------|------|
| **CLI** | Streaming stats, filters, unique values, and JSON export on large files |
| **TUI** | Interactive table exploration with progressive loading |
| **Web UI** | Same exploration model in a browser via a local HTTP server |

Interactive UIs share one core model (`AppModel`) so terminal and browser behavior stay aligned.

## Design goals

1. **Fast initial paint** — show headers and the first rows immediately; grow the row count in the background.
2. **Predictable CSV parsing** — RFC 4180 via the `csv` crate; preview uses mmap and on-demand record parse.
3. **One shared core** — `csv-utils-core` owns parsing, preview, CLI engine, and view state; frontends stay thin.
4. **Progressive disclosure** — CLI for scripts; TUI/web when you need to look around a file.
5. **Honest limits** — document mmap, indexing, and progressive-stats constraints; see [limitations](reference/limitations.md).

## UX values

- **Keyboard-first in the TUI** — mouse augments (select, scroll, resize, drag panels); it does not replace keys.
- **Parity where it matters** — TUI and web share selection, scrolling, column list, column info, row JSON, and column resize semantics.
- **Terminal-native web defaults** — browser UI follows system light/dark; an explicit theme override is available.
- **Fixed-width cells for scanning** — monospace columns auto-fit to header and loaded content (4–64 chars). Text and dates truncate with middle `...`; numbers rescale instead of ellipsis.

## Non-goals

- Full CSV dialect configuration (custom delimiters, etc.).
- Persisted UI state across sessions (column widths, selection, scroll, filters).
- A single in-process cache shared between CLI and TUI invocations.

See [known limitations](reference/limitations.md) for the full list.
