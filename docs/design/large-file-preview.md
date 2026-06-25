# Design: large-file CSV preview

Status: **approved design** (not yet implemented).

The TUI and web UIs load CSV files through a preview pipeline in
`csv-utils-core/src/preview.rs`. Today that path keeps every body line as an owned
`String` and does too much per-row work on the render thread. This doc describes
the target loader: **`csv` crate parsing**, **mmap storage**, and a **record
byte-offset index**.

## Problems today

1. **Memory** — the background scan pushes each line into `Vec<String>`. Memory
   grows with file size and does not scale past roughly 1 GB.
2. **Responsiveness** — `maybe_update_column_layout()` runs on every TUI/web
   frame, re-splitting new rows across all columns and updating width, type
   inference, and stats for every column even when the user is not viewing them.

## Target architecture

Three layers that work together:

| Layer | Role | Mechanism |
|---|---|---|
| **Storage** | Raw file bytes without decoding the whole file | Read-only **mmap** (`memmap2`) |
| **Index** | O(1) lookup: row → byte range | `Vec<u64>` record start offsets |
| **Parse** | RFC 4180 fields (quotes, embedded newlines) | Rust [`csv`](https://docs.rs/csv) crate |

```text
  file on disk
       │
       ▼
  mmap (OS-paged; RSS tracks pages actually read)
       │
       ├── background: csv::Reader over &mmap[..]
       │       └── each read_record → push reader.position().byte() to index
       │
       └── on demand (visible row N):
               slice = mmap[offset[N] .. offset[N+1]]   (or ..EOF for last row)
               csv::ReaderBuilder::new().from_reader(slice)
               read one record → fields for display / metadata
```

- **mmap** — file bytes are mapped, not copied into a giant in-memory buffer.
- **Offset index** — random access to row N without scanning from the start.
- **`csv` crate** — correct record boundaries and field splitting; used both
  while building the index and when parsing a single row for the viewport.

## Building the offset index

Do **not** index by scanning for `\n` bytes. Newlines inside quoted fields would
desynchronize row numbers from CSV records.

During the background scan:

1. Open `csv::Reader` on `&mmap[..]`.
2. Before each `read_byte_record` / `read_record`, record `reader.position().byte()`.
3. Append to `record_offsets`; body row count = index length.

Headers stay separate (current model): parse the header row once at open; index
body records only.

## Memory footprint

| Structure | Size |
|---|---|
| mmap mapping | Virtual ≈ file size; physical RAM only for touched pages |
| `record_offsets` | ~8 bytes × row count |
| Decoded cells | Visible viewport only (+ optional small LRU cache) |

For very large row counts, the index itself may need compaction (`u32` offsets,
chunked index, or sparse sampling) — revisit if that becomes a bottleneck.

## Loading behavior

Keep the current progressive UX:

| Phase | Work |
|---|---|
| **Sync (startup)** | mmap file, parse header, read first N body records for instant paint |
| **Background** | Continue sequential `csv` reads; grow `record_offsets`; update summaries |
| **Render** | Parse only rows in the current viewport via index + `csv` |

The UI continues to show row count and `loading…` until the background scan
finishes (`scan_done`).

## Column metadata

| Metadata | When computed |
|---|---|
| Record offsets, row count | Background scan (eager) |
| Headers, column count | At open (eager) |
| Cell values | On demand for visible rows |
| Column width auto-fit | Progressive: visible window + sample; widen as scan advances |
| Type inference | Sample + visible rows; refine in background |
| Column statistics | **Lazy:** only for the column open in the info panel; refine as scan progresses |
| Filters (future) | Background job building a list of matching row indices |

Move ingestion and metadata accumulation off the render thread. The UI reads
snapshots (e.g. `Arc` swap or channel) and never blocks on full-file work per
frame.

Stats over a partial scan stay labeled partial (e.g. “Statistics from loaded
rows only”). Exact min/max/distinct finalize when the scan completes; for very
large columns, bounded sketches (e.g. HyperLogLog for distinct counts) are an
optional later optimization.

## Implementation phases

1. **Replace `schema::split_row` with `csv`** — same behavior and tests; enables
   correct embedded newlines.
2. **Record offset index** — stop storing `Vec<String>` rows; slice from an
   in-memory buffer first.
3. **mmap** — map the file read-only once indexing works on a owned buffer.
4. **Background worker + lazy column stats** — ingestion and summaries off the
   render thread; stats only when the info panel is open.
5. **Scale optimizations (optional)** — segment summaries, HLL/reservoir
   sketches for bounded-memory stats on huge columns.

## Future option: Apache Arrow

[Apache Arrow](https://arrow.apache.org/) may be useful later for **vectorized
batch statistics** over fixed-size row segments (min/max, type checks). It is
not part of the core loader design; the interactive path stays mmap + index +
on-demand `csv` parsing.

## Caveats

- **Read-only viewing** — mmap assumes the file is not truncated or modified
  underneath the mapping while open.
- **Index correctness** — offsets must come from `csv` reader positions, not
  raw newline search.
- **Partial stats** — UI must distinguish in-progress vs final scan results.

## Related

- [Data loading](../reference/data-loading.md) — current preview pipeline
- [Architecture](../architecture.md)
- [Known limitations](../reference/limitations.md)
