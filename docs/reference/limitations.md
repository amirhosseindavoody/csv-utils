# Known limitations

Intentional trade-offs and current constraints. When something here is removed, update this page and [principles](../principles.md#non-goals) if needed.

## Data access

- The preview path is **read-only**. mmap assumes the file is not modified or truncated while open.
- The record offset index grows with row count (~8 bytes per row).
- Type inference and column auto-fit use **indexed** rows only. Rows the background scan has not reached yet do not affect them.
- Row navigation is limited to indexed rows until the background scan finishes.
- CLI commands re-open files; there is no shared cache with an open TUI/web session.

## Filtering and sorting

- Changing a column filter or hiding/unhiding rows rebuilds the matching-row cache in full. While the background scan only appends new rows, the cache **extends** in O(Δ). With an active sort, extending still rebuilds the sorted order with one parse per matching row.
- There is no per-column index for filters. Very large files with frequent filter edits still pay a linear scan on each full rebuild. See [Row filtering](../design/row-filtering.md).
- Fuzzy text filters rescore cells on each rebuild; numeric expressions are re-parsed from their string form on each rebuild.
- Row filters, hidden rows/columns, multi-select, and sort are **session-only**. They reset when a file is closed or reopened.
- Hidden rows are omitted from the table and from row navigation. The title bar shows `visible/total rows` when rows are hidden or filtered.

## Column info and layout

- Column statistics accumulate during load. Opening the info panel mid-scan can show partial stats until the scan finishes.
- Manual column resize locks that column’s width until a new file is opened. Widths are not persisted across sessions.

## Other

- The web UI layout is fixed from the terminal size at `:web` handoff; it does not track later terminal or browser resize on the server.
- No custom CSV dialect (delimiter, quote style, etc.): comma-separated with standard quoting.

For how the interactive path stays responsive during scan/filter/sort, see [Performance & TUI responsiveness](../design/performance-tui-responsiveness.md).
