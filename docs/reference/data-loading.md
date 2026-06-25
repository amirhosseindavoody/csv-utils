# Data loading

How TUI and web UIs load CSV data. CLI uses a separate streaming path; see [CLI](../features/cli.md).

## Preview pipeline

1. **Sync:** mmap the file read-only, parse the header, index the first **128** body
   records (`INITIAL_BODY_LINES`).
2. **Background thread:** continue sequential `csv` reads; append record byte offsets
   and update column width / type inference (`ColumnLayoutState`).
3. **Render:** parse fields on demand for visible rows only (`PreviewData::row_fields`).

Headers are available immediately. Row count in the title/status grows until
`scan_done`.

## Storage model

| Piece | Location | Notes |
|---|---|---|
| File bytes | `memmap2` read-only map | OS-paged; not copied into `Vec<String>` |
| Row index | `record_offsets: Vec<u64>` | Body record start bytes from `csv` reader position |
| Parsed cells | Ephemeral | Built per viewport row via `csv` on a mmap slice |

See [large-file preview design](../design/large-file-preview.md) for the full picture.

## APIs

| API | Use |
|-----|-----|
| `PreviewData::load_header_and_initial_rows` | TUI/web startup |
| `PreviewData::start_background_scan` | Continue indexing after sync batch |
| `PreviewData::row_fields(index)` | Parse one body row on demand |
| `PreviewData::layout()` | Shared `ColumnLayoutState` (width, inference, lazy stats) |
| `PreviewData::load_limited` | Tests (`scan_done = true`) |

Location: `csv-utils-core/src/preview.rs`, `csv-utils-core/src/column_layout.rs`

## Threading

`AppModel` holds an optional `scan_thread` join handle and a cancel flag. The background thread
updates `record_offsets` and `ColumnLayoutState`. On TUI quit the scan is cancelled and
abandoned without blocking the shell (`abandon_scan_thread`). When switching files (`reopen`,
`:close`) the previous scan is cancelled and joined (`join_scan_thread`).

Column statistics backfill runs on the UI thread when the info panel is open
(budget: 512 rows per `maybe_update_column_layout` call).

### Interaction with row-filter cache

`TableViewState` caches the filtered row index list (`cached_matching_rows`) and
records the row count at cache build time (`cached_row_count`). When the
background scan adds new rows, the next call to `matching_row_indices` detects
`cached_row_count != preview.row_count()` and rebuilds the cache. The rebuild
happens at most once per event-loop tick (inside `maybe_update_column_layout`),
so the rendering cost of filtering scales with tick rate, not with the number of
draw calls per tick. See [Row filtering design](../design/row-filtering.md) for
the full caching strategy.

## Status display

| State | TUI title | Web meta |
|-------|-----------|----------|
| Scanning | `loading…` | poll continues |
| Done | (no badge) | `loaded` in status line |
| Error | `ERROR` | `error` in status line |

## I/O notes

- Run from repo root for `test-data/…` paths in pixi tasks
- Embedded newlines inside quoted fields are supported (RFC 4180 via `csv` crate)

## Related

- [Large-file preview design](../design/large-file-preview.md)
- [Row filtering design](../design/row-filtering.md)
