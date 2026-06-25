# Known limitations

Current constraints and intentional trade-offs.

## Data access

- Preview path is **read-only**; mmap assumes the file is not modified or truncated while open.
- The record offset index grows with row count (~8 bytes per row); extremely large files may need index compaction in the future.
- Type inference and auto-fit use **indexed rows**; tail rows not yet scanned do not affect them until the background scan reaches them.
- Row navigation is limited to indexed rows until the background scan completes.
- CLI commands re-open files; no shared cache with TUI/web sessions.

## Filtering

- The row-filter cache is rebuilt whenever any column filter changes or new rows arrive from the background scan. For very large row counts (millions), the rebuild is still a linear scan of all indexed rows; no index structure is maintained per-column. See [Row filtering design](../design/row-filtering.md).
- Fuzzy text row filtering (`text_cell_matches`) rescores every cell in the column on each rebuild. It does not cache per-cell scores between filter edits.
- Numeric filter expressions are re-parsed from their string representation on every cache rebuild. For compound expressions (`(>=10) & (<20) | (==0)`) this is negligible; if pathological nesting becomes a concern, parsed `Expr` trees could be stored alongside the expression string.
- Row filters are session-only; they are not persisted in `csv-utils.json` and reset when a file is closed or reopened.

## Column info and statistics

- Column statistics are computed **only while the info panel is open**; opening the panel mid-scan backfills incrementally (512 rows/frame budget).
- Manual column resize locks width for that column until a new file is opened; widths are not persisted across sessions.

## Other

- JSON CLI output does not escape embedded quotes in values.
- Web UI uses fixed layout constants (not terminal/window resize aware on the server side).
- No custom CSV dialect configuration (delimiter, etc.); comma-separated with standard quoting.

When a limitation is removed, update this file and [principles](../principles.md#non-goals-for-now) if applicable.
