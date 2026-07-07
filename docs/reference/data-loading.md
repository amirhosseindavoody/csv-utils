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
| `PreviewData::layout()` | Shared `ColumnLayoutState` (width, inference, stats) |
| `PreviewData::load_limited` | Tests (`scan_done = true`) |

Location: `csv-utils-core/src/preview.rs`, `csv-utils-core/src/column_layout.rs`

## Threading

`AppModel` holds an optional `scan_thread` join handle and a cancel flag. The background thread
updates `record_offsets` and `ColumnLayoutState`. On TUI quit, file close, or file switch the
scan is cancelled and abandoned without blocking the shell (`abandon_scan_thread`). The
background thread exits on its own after observing the cancel flag.

Column statistics are updated on the background scan thread (and during the
initial row load) as rows are indexed.

### Closing/switching files: dropping large stats off the UI thread

A fully-scanned wide/tall CSV can accumulate millions of small `String`
allocations in `ColumnStatsAccum::distinct` (up to `DISTINCT_CAP` = 10,000
per column, tracked for *every* column regardless of inferred type — see
`csv-utils-core/src/column_stats.rs`). Freeing that many small allocations
takes real time: benchmarking a 10,000-row × 1,000-column file
(`cargo run --release --example bench_stats_drop -p csv-utils-core`) showed
~940ms just to `drop` the populated `ColumnLayoutState`.

`AppModel::close_file` and `AppModel::reopen` replace `self` with a fresh
`AppModel` (`Self::open(...)`). Naively doing `*self = Self::open(...)?`
drops the *old* `AppModel` — and therefore the old `ColumnLayoutState` —
synchronously on the calling thread. Since pressing `q` with a file open
always calls `close_file` before quitting (see `csv-utils/src/tui/app.rs`),
this made quitting feel like it hung for up to a second on large files, even
though the background scan itself was already cancelled promptly
(`abandon_scan_thread`, not `join_scan_thread`).

Both methods now use `AppModel::replace_and_discard`, which does
`std::mem::replace` and hands the old value to a detached
`std::thread::spawn(move || drop(old_model))`. The caller (UI thread) never
waits on the deallocation, so closing/switching files — and quitting — stays
under ~100ms regardless of how much stats state had accumulated. See
`model::tests::close_file_does_not_block_on_large_accumulated_stats` for the
regression test.

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
