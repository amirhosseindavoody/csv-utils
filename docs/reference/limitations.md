# Known limitations

Current constraints and intentional trade-offs.

- TUI/web hold all body lines in memory as raw strings (not suitable for multi-GB files without paging).
- Type inference scans **loaded rows only**; very long values in unscanned tail rows may not affect auto-fit until loaded.
- Manual column resize locks width for that column until a new file is opened; widths are not persisted across sessions.
- CLI commands re-open files; no shared cache with TUI/web sessions.
- Row navigation is limited to loaded lines until the background scan completes.
- JSON CLI output does not escape embedded quotes in values.
- Web UI uses fixed layout constants (not terminal/window resize aware on the server side).
- CSV parsing supports quoted fields but not full dialect configuration (delimiter, etc.).

When a limitation is removed, update this file and [principles](../principles.md#non-goals-for-now) if applicable.
