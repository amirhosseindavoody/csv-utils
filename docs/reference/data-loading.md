# Data loading

How TUI and web UIs load CSV data. CLI uses a separate streaming path; see [CLI](../features/cli.md).

## Preview pipeline

1. **Sync:** read header + first **128** body lines as raw UTF-8 (`INITIAL_BODY_LINES`).
2. **Background thread:** append remaining body lines to `PreviewData`.
3. **Render:** call `split_row` only on visible rows.

Headers are available immediately. Row count in the title/status grows until `scan_done`.

## APIs

| API | Use |
|-----|-----|
| `PreviewData::load_header_and_initial_rows` | TUI/web startup |
| `PreviewData::start_background_scan` | Background append |
| `PreviewData::load_limited` | Tests (`scan_done = true`) |

Location: `csv-utils-core/src/preview.rs`

## I/O details

- 1 MiB `BufReader`, `\n`-delimited lines
- Body lines stored as raw strings in memory (see [limitations](limitations.md))
- Run from repo root for `test-data/…` paths in pixi tasks

## Threading

`AppModel` holds an optional `scan_thread` join handle. TUI and web server join the thread on exit (`join_scan_thread`).

## Status display

| State | TUI title | Web meta |
|-------|-----------|----------|
| Scanning | `loading…` | poll continues |
| Done | (no badge) | `loaded` in status line |
| Error | `ERROR` | `error` in status line |
