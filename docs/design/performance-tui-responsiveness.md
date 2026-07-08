# Design: performance & TUI responsiveness

Status: **partially implemented** — P0 items below are in tree; P1–P3 remain
proposals. Based on a review of architecture, preview loading, the shared view
model, and the TUI event loop as of `2026.6.30+0`.

This note prioritizes changes that keep the interactive UI responsive while
files are scanning, filtering, or sorting. It builds on existing mitigations
documented in [large-file preview](large-file-preview.md),
[row filtering](row-filtering.md), and [data loading](../reference/data-loading.md).

## Current strengths

The interactive path already follows the right high-level shape:

| Mechanism | Why it helps |
|-----------|--------------|
| mmap + offset index + on-demand `csv` parse | Fast first paint; RSS tracks touched pages |
| Background scan after first 128 rows | UI can navigate early |
| Row-filter cache (≤1 rebuild per draw) | Avoids O(R×F) per draw call site |
| **Incremental matching-row cache** | New scan rows append in O(Δ), not O(R) |
| Sorted-row cache + **precomputed sort keys** | One parse per row, then in-memory sort |
| **Dirty-flag TUI redraw** | Draw on input / resize / throttled scan progress |
| Detached drop of old `AppModel` on close | Quit/switch no longer blocks on stats free |
| Web `ClientView` snapshot behind `RwLock` | `GET /api/state` does not hold the model lock |

## Hot path today

```text
TUI loop:
  if needs_redraw:
    lock AppModel
    maybe_update_column_layout()
      → apply_fitted_column_widths()
      → matching_row_indices()   # append Δ rows, or full rebuild if invalidated
    terminal.draw()
    unlock
  poll(50ms)
  on input/resize → needs_redraw = true
  on scan progress (throttled ≥100ms) → needs_redraw = true
```

Background scan (per indexed row):

```text
lock PreviewInner → push offset
lock ColumnLayoutState → observe_fields (width + infer + stats)
```

## Priority ranking

Impact is judged by how often the cost hits the **UI thread** and how it scales
with row count `R`, column count `C`, viewport size `V`, and active filters `F`.

| Priority | Theme | Status |
|----------|-------|--------|
| **P0** | Dirty-flag / throttled redraw | **Done** (`tui/app.rs`) |
| **P0** | Incremental filter/sort cache during scan | **Done** (`model.rs`) |
| **P0** | Sort key precomputation | **Done** (`sort.rs` / `model.rs`) |
| **P1** | Cheaper `row_fields` + less lock contention | Proposal |
| **P1** | Filter eval micro-costs | Proposal |
| **P2** | Draw-path allocations & layout locks | Proposal |
| **P2** | Stats observe / distinct cost | Proposal |
| **P3** | Web snapshot / API lock duration | Proposal |

---

## P0 — Event loop: draw only when needed ✅

### Problem (was)

`csv-utils/src/tui/app.rs` redraws on **every** loop iteration. The 50ms poll
timeout meant ~20 FPS even when nothing changed. `last_redraw` was updated when
the scan was active, but it did **not** gate drawing.

### Implemented

1. `needs_redraw` is set by key/mouse handlers, terminal resize, and scan
   progress (row count / `scan_done` / `scan_error` changed).
2. Draw + `maybe_update_column_layout` run only when `needs_redraw` is true.
3. While scanning, progress-driven redraws are capped at **100ms** intervals.
4. Idle poll remains 50ms so input stays responsive; no draw work runs between
   events when the scan is done and nothing changed.

---

## P0 — Incremental filter (and sort) cache while scanning ✅

### Problem (was)

`matching_row_indices` treated any `cached_row_count != preview.row_count()` as
full invalidation. During background scan the UI rebuilt the entire matching-row
`Vec` (and re-sorted) on nearly every frame.

### Implemented

1. **Append-only when only new rows arrived:** keep
   `cached_matching_rows` for `0..cached_row_count`, evaluate
   `cached_row_count..current_count`, append survivors.
2. **Sorted scrollable cache:** with no sort, append new non-pinned matches;
   with sort active, rebuild via precomputed keys (next item).
3. Full O(R) rebuild still runs when filters/hides invalidate the cache
   (`cached_matching_rows = None`).

See [row filtering](row-filtering.md) for the updated contract.

---

## P0 — Sort: extract keys once, then sort indices ✅

### Problem (was)

`rebuild_sorted_scrollable_cache` used `sort_by` where each comparison called
`row_fields` **twice** — O(R log R) full-row CSV parses on the UI thread.

### Implemented

1. `sort::sort_indices_by_cells` precomputes a `SortKey` per row from a
   caller-supplied cell list, then sorts keys in memory.
2. `rebuild_sorted_scrollable_cache` does **one** `row_fields` per scrollable
   row, then calls that helper.
3. `set_sort_column` / `clear_sort` / pin toggles explicitly rebuild the sorted
   cache (matching-row cache may still be valid).

---

## P1 — `row_fields`: less contention, less work per call

### Problem

Every `row_fields(i)` takes the exclusive `PreviewInner` mutex, runs a fresh
`csv::Reader` on the record slice, and allocates `Vec<String>`. Call sites
multiply this: viewport draw (~V cells), filter rebuild (R rows), sort
(now one pass, still R parses). Preview and layout both use `Mutex`, so the
background scan’s per-row locks contend with UI reads.

### Suggestions

| Change | Detail |
|--------|--------|
| **`RwLock` for `PreviewInner` / layout** | UI reads share; scan still writes. Rename `with_read_lock` to match reality or switch to real read locks. |
| **Batch offset appends** | Background scan buffers e.g. 64–256 offsets, then one lock to `extend`. Same for layout: `observe_fields_batch`. |
| **Reuse shared mmap** | Pass `Arc<Mmap>` into the scan thread instead of remapping the file. |
| **Viewport row cache** | LRU or ring cache of recently parsed rows (size ≈ 2–3× viewport). Invalidate on file close only. |
| **Column-oriented extract** | `row_field(index, col) -> Option<String>` or borrow into a thread-local `ByteRecord` to avoid allocating every field when only one column is needed (filters, sort). |
| **`headers()` without clone** | Return `Arc<[String]>` or copy under a short lock into a cached `Arc` on the model. |

### Expected effect

Lower lock wait during scan; cheaper scroll/filter/sort when only one column is
needed; fewer allocations in the draw path when the same rows are re-shown.

---

## P1 — Filter evaluation micro-costs

### Problem

Documented in [limitations](../reference/limitations.md) and still true:

- Numeric filter strings are **re-parsed for every row** in
  `numeric_cell_matches`.
- Fuzzy scoring allocates two `Vec<char>` per cell.
- `row_passes_value_filters` parses the **entire row** even when one column is
  filtered.

### Suggestions

1. Store a parsed `Expr` (or `Arc<Expr>`) next to each numeric filter string;
   validate once at set time (already done) and evaluate the AST thereafter.
2. For fuzzy row filters, lowercase/query-prepare once; compare with byte/ASCII
   fast paths when both sides are ASCII (common for CSV).
3. Use single-column extract (P1) inside `row_passes_value_filters`.
4. Longer term (only if R is routinely millions): maintain per-column posting
   lists or sorted numeric indexes — out of scope until incremental cache (P0)
   is insufficient.

### Expected effect

Makes each cache rebuild cheaper; compounds with incremental rebuild.

---

## P2 — Draw path: fewer allocations and locks per cell

### Problem

Per visible cell, draw typically:

1. locks layout for `effective_column_kind` (Auto columns)
2. formats via `format_cell_for_column` / `truncate_middle` / `sanitize_ascii`
3. allocates several short-lived `String`s

`headers()` clones, `scrollable_table_rows` / `visible_table_rows` often clone
index vectors, and border drawing can do O(rows×cols) work with repeated
membership checks.

### Suggestions

1. Snapshot column kinds (and widths) once per frame into a small local array
   before the cell loop — one layout lock per frame.
2. Fast-path ASCII: if `cell.is_ascii()`, skip `sanitize_ascii` allocation;
   truncate with byte/char indexing in place where safe.
3. Avoid cloning matching-row caches in draw: pass slices; only clone when
   ownership is required.
4. Precompute border X positions as a `HashSet` or sorted list **once** per
   frame outside the inner loop.
5. Hit-testing: reuse the same layout snapshot as the last draw (store
   `col_indices`, row window, and x offsets on `Areas` / a `HitTestCache`) so
   mouse drag does not redo viewport assembly from scratch.

### Expected effect

Lower steady-state CPU when navigating a fully loaded file; smoother mouse
interaction.

---

## P2 — Background stats / layout observe cost

### Problem

`observe_fields` runs width + inference + full `ColumnStatsAccum::observe` for
**every** column on **every** indexed row. Distinct tracking allocates up to
`DISTINCT_CAP` (10 000) strings per column. `sanitize_ascii` allocates during
width tracking. Wide × tall files make the scan thread heavy and increase
contention on the layout mutex; drop cost was mitigated for close, but live
memory and scan CPU remain.

### Suggestions

1. Defer or sample distinct-value tracking (e.g. only first N rows, or only when
   column info is opened / column is selected).
2. Track distinct with a hash of the cell (or `ahash` + capped intern) instead of
   storing every string when only the count is shown.
3. Skip type probes that are already ruled out by `ColumnInferState` (once
   inferred as text, stop date/int/float checks in stats if the UI only needs
   text stats).
4. Compute `sanitize_ascii` length without allocating when the cell is already
   printable ASCII.
5. Batch layout updates (P1) so the UI can read widths/kinds between batches.

### Expected effect

Faster scans, less RAM, shorter layout lock hold times → fewer UI stalls while
`loading…` is shown.

---

## P3 — Web path alignment

### Problem

After `:web`, a tokio task rebuilds `client_view` every 150ms while scanning —
same filter/sort/parse costs as the TUI draw, under the `AppModel` mutex.
`POST /api/action` also builds a full snapshot while holding the lock.

### Suggestions

1. Apply the same dirty/throttle and incremental cache rules as the TUI.
2. Build `ClientView` off the hot lock when possible: copy a thin snapshot
   struct under the mutex, format cells after releasing (or reuse the last
   snapshot’s cell grid when only row_count meta changed).
3. Keep `GET /api/state` on the `RwLock` snapshot (already good).

---

## Suggested implementation order

1. ~~**Dirty-flag + scan-throttled redraw** (P0)~~ — done
2. ~~**Identity / incremental matching-row cache** (P0)~~ — done (incremental append)
3. ~~**Sort key precomputation** (P0)~~ — done
4. **Parsed numeric filter AST + single-column field extract** (P1)
5. **RwLock + batched scan updates + viewport row cache** (P1)
6. **Per-frame kind snapshot + ASCII format fast path** (P2)
7. **Stats observe trimming** (P2)
8. **Web snapshot throttle / unlock** (P3)

## Measurement ideas

Add or extend benches/examples (alongside `bench_stats_drop`) for:

| Scenario | Metric |
|----------|--------|
| Event loop idle, scan done | CPU % / draws per second |
| Scan 1M×20 with no filter | p95 time in `matching_row_indices` per tick |
| Same with one text filter | rebuild time; UI poll latency |
| `:sort` on 100k rows | wall time of `rebuild_sorted_scrollable_cache` |
| Scroll viewport 30×15 | time in `draw_table` / `row_fields` |

Prefer release builds via Pixi (`pixi run -- cargo bench` / examples).

## Doc updates when implementing

When any proposal ships, update in the same change:

- This file’s status line and check off the item
- [row-filtering.md](row-filtering.md) / [large-file-preview.md](large-file-preview.md) if cache or scan contracts change
- [limitations.md](../reference/limitations.md) when a listed constraint is removed
- [architecture.md](../architecture.md) if locking or threading changes
- [tui.md](../features/tui.md) only if user-visible timing/behavior changes
- Bump “Last verified against” in [index.md](../index.md)

## Related

- [Architecture](../architecture.md)
- [Data loading](../reference/data-loading.md)
- [Large-file preview](large-file-preview.md)
- [Row filtering](row-filtering.md)
- [Known limitations](../reference/limitations.md)
