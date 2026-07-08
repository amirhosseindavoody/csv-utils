# Design: large-file CSV preview

Status: **implemented** in `csv-utils-core` (TUI and web preview path).

The TUI and web UIs load CSV files through a preview pipeline that maps the file
read-only, indexes record byte offsets, and parses rows on demand with the Rust
[`csv`](https://docs.rs/csv) crate.

## Architecture

Three layers:

| Layer | Role | Mechanism |
|---|---|---|
| **Storage** | Raw file bytes without decoding the whole file | Read-only **mmap** (`memmap2`) |
| **Index** | O(1) lookup: row → byte range | `Vec<u64>` record start offsets |
| **Parse** | RFC 4180 fields (quotes, embedded newlines) | `csv` on a byte slice per record |

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

Location: `csv-utils-core/src/preview.rs`

## Offset index

Offsets come from `reader.position().byte()` during sequential `csv` reads — not
from scanning for `\n` bytes (which breaks when fields contain embedded newlines).

The header row is parsed once at open; the index covers **body records only**.

## Memory footprint

| Structure | Size |
|---|---|
| mmap mapping | Virtual ≈ file size; physical RAM only for touched pages |
| `record_offsets` | ~8 bytes × row count |
| Decoded cells | Visible viewport only (parsed on demand) |

For very large row counts, the index itself may need compaction — revisit if that
becomes a bottleneck.

## Loading behavior

| Phase | Work |
|---|---|
| **Sync (startup)** | mmap file, parse header, index first **128** body records (`INITIAL_BODY_LINES`) |
| **Background** | Continue sequential `csv` reads; grow `record_offsets`; update column layout |
| **Render** | Parse only rows in the current viewport via index + `csv` |

The UI shows row count and `loading…` until the background scan finishes
(`scan_done`).

## Column metadata

Width auto-fit and type inference run on a **background thread** while records
are indexed (`column_layout.rs`, updated from `preview.rs`).

Column **statistics** are computed on the **background scan thread** (and during
the initial row load) as rows are indexed, so they are ready when the column
info panel opens (`c` in TUI/web). Stats over a partial scan are labeled partial
in the panel.

| Metadata | When computed |
|---|---|
| Record offsets, row count | Background scan |
| Headers, column count | At open |
| Cell values | On demand for visible rows |
| Column width auto-fit | Background scan (progressive) |
| Type inference | Background scan (progressive) |
| Column statistics | Background scan (progressive) |
| Filtered row index list | Once per tick in `maybe_update_column_layout`; cache invalidated on filter change or new rows |

## CLI path

CLI commands (`engine.rs`, `unique.rs`) still stream the file line-by-line and
use `schema::split_row` (backed by the `csv` crate). They do not use mmap or the
offset index.

## Future option: Apache Arrow

[Apache Arrow](https://arrow.apache.org/) may be useful later for **vectorized
batch statistics** over fixed-size row segments. It is not part of the preview
loader; the interactive path stays mmap + index + on-demand `csv` parsing.

## Caveats

- **Read-only viewing** — mmap assumes the file is not truncated or modified
  underneath the mapping while open.
- **Partial stats** — the info panel distinguishes in-progress vs final scan
  results.

## Related

- [Data loading](../reference/data-loading.md)
- [Architecture](../architecture.md)
- [Row filtering design](row-filtering.md) — filter cache and its interaction with the background scan
- [Performance & TUI responsiveness](performance-tui-responsiveness.md) — proposed scan/lock/redraw improvements
- [Known limitations](../reference/limitations.md)
