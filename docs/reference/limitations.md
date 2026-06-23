# Known limitations

Current constraints and intentional trade-offs.

- TUI/web hold all body lines in memory as raw strings (not suitable for multi-GB files without paging).
- Column types are name-prefix heuristics, not value inference.
- CLI commands re-open files; no shared cache with TUI/web sessions.
- Column widths reset when the TUI/web session restarts (not persisted).
- Row navigation is limited to loaded lines until the background scan completes.
- JSON CLI output does not escape embedded quotes in values.
- Web UI uses fixed layout constants (not terminal/window resize aware on the server side).
- CSV parsing supports quoted fields but not full dialect configuration (delimiter, etc.).

When a limitation is removed, update this file and [principles](../principles.md#non-goals-for-now) if applicable.
