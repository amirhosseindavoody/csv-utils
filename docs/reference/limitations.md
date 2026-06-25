# Known limitations

Current constraints and intentional trade-offs.

- Preview path is **read-only**; mmap assumes the file is not modified or truncated while open.
- The record offset index grows with row count (~8 bytes per row); extremely large files may need index compaction in the future.
- Type inference and auto-fit use **indexed rows**; tail rows not yet scanned do not affect them until the background scan reaches them.
- Column statistics are computed **only while the info panel is open**; opening the panel mid-scan backfills incrementally.
- Manual column resize locks width for that column until a new file is opened; widths are not persisted across sessions.
- CLI commands re-open files; no shared cache with TUI/web sessions.
- Row navigation is limited to indexed rows until the background scan completes.
- JSON CLI output does not escape embedded quotes in values.
- Web UI uses fixed layout constants (not terminal/window resize aware on the server side).
- No custom CSV dialect configuration (delimiter, etc.); comma-separated with standard quoting.

When a limitation is removed, update this file and [principles](../principles.md#non-goals-for-now) if applicable.
