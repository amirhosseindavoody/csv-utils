# Known limitations

Current constraints and intentional trade-offs.

## Data access

- Preview path is **read-only**; mmap assumes the file is not modified or truncated while open.
- The record offset index grows with row count (~8 bytes per row); extremely large files may need index compaction in the future.
- Type inference and auto-fit use **indexed rows**; tail rows not yet scanned do not affect them until the background scan reaches them.
- Row navigation is limited to indexed rows until the background scan completes.
- CLI commands re-open files; no shared cache with TUI/web sessions.

## Filtering

- The row-filter cache is rebuilt in full when any column filter changes or rows
  are hidden/unhidden. When the background scan only appends newly indexed rows,
  the cache **extends** in O(Δ) rather than rescanning all indexed rows. With an
  active sort, extending still rebuilds the sorted scrollable order with one
  parse per matching row (precomputed keys). For very large row counts with
  frequent filter edits, each full rebuild is still a linear scan; no per-column
  index structure is maintained. See [Row filtering design](../design/row-filtering.md)
  and [Performance & TUI responsiveness](../design/performance-tui-responsiveness.md).
- Fuzzy text row filtering (`text_cell_matches`) rescores every cell in the column on each rebuild. It does not cache per-cell scores between filter edits.
- Numeric filter expressions are re-parsed from their string representation on every cache rebuild. For compound expressions (`(>=10) & (<20) | (==0)`) this is negligible; if pathological nesting becomes a concern, parsed `Expr` trees could be stored alongside the expression string.
- Row filters and hidden rows are session-only; they are not persisted in settings files and reset when a file is closed or reopened.
- Hidden rows are excluded from the table and from row navigation; the title bar shows `visible/total rows` when any rows are hidden or filtered.

## Column info and statistics

- Column statistics are computed progressively during file load (initial rows + background scan); opening the info panel mid-scan may show partial stats until the scan finishes.
- Manual column resize locks width for that column until a new file is opened; widths are not persisted across sessions.

## Other

- JSON CLI output does not escape embedded quotes in values.
- Web UI uses fixed layout constants (not terminal/window resize aware on the server side).
- No custom CSV dialect configuration (delimiter, etc.); comma-separated with standard quoting.

When a limitation is removed, update this file and [principles](../principles.md#non-goals-for-now) if applicable.

For prioritized ideas to address interactive lag (redraw rate, incremental
filter cache, sort key precomputation, lock contention), see
[Performance & TUI responsiveness](../design/performance-tui-responsiveness.md).
