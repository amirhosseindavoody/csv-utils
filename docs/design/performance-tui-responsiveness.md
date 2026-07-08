# Design: performance & TUI responsiveness

Status: **proposals** (not yet implemented). Based on a review of architecture,
preview loading, the shared view model, and the TUI event loop as of
`2026.6.30+0`.

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
| Row-filter cache (≤1 rebuild per tick) | Avoids O(R×F) per draw call site |
| Sorted-row cache alongside filter cache | Sort not recomputed every draw |
| Detached drop of old `AppModel` on close | Quit/switch no longer blocks on stats free |
| Web `ClientView` snapshot behind `RwLock` | `GET /api/state` does not hold the model lock |

The remaining issues are mostly **amplifiers**: work that is acceptable once
becomes expensive when the event loop redraws unconditionally, or when cache
invalidation fires every tick while `row_count` grows.

## Hot path today

```text
TUI loop (~20 Hz, always):
  lock AppModel
  maybe_update_column_layout()
    → apply_fitted_column_widths()
    → matching_row_indices()          # may O(R) or O(R×F); may re-sort
  terminal.draw()
    → visible_table_rows / row_fields / format_column_cell per cell
  unlock
  poll(50ms)
```

Background scan (per indexed row):

```text
lock PreviewInner → push offset
lock ColumnLayoutState → observe_fields (width + infer + stats)
```

## Priority ranking

Impact is judged by how often the cost hits the **UI thread** and how it scales
with row count `R`, column count `C`, viewport size `V`, and active filters `F`.

| Priority | Theme | Typical symptom |
|----------|-------|-----------------|
| **P0** | Dirty-flag / throttled redraw | Idle CPU; lag while scanning |
| **P0** | Incremental filter/sort cache during scan | Stutter every ~50ms on large files |
| **P0** | Sort key precomputation | `:sort` freezes on large filtered sets |
| **P1** | Cheaper `row_fields` + less lock contention | Scroll/filter feel sticky |
| **P1** | Filter eval micro-costs | Filter apply / scan rebuild slow |
| **P2** | Draw-path allocations & layout locks | High CPU at steady state |
| **P2** | Stats observe / distinct cost | Slow scan; heavy close (partially mitigated) |
| **P3** | Web snapshot / API lock duration | Browser UI lag during scan |

---

## P0 — Event loop: draw only when needed

### Problem

`csv-utils/src/tui/app.rs` redraws on **every** loop iteration. The 50ms poll
timeout means ~20 FPS even when nothing changed. `last_redraw` is updated when
the scan is active, but it does **not** gate drawing — so the intended throttle
is incomplete.

### Suggestion

1. Introduce a `needs_redraw: bool` (or generation counter) set by:
   - any key/mouse handler that mutates state
   - terminal resize
   - scan progress (row count / `scan_done` / `scan_error` changed)
2. Draw only when `needs_redraw` is true.
3. While scanning, refresh at a capped rate (e.g. 5–10 Hz), not every 50ms.
4. On idle with `scan_done`, block on `event::poll` with a longer timeout (or
   wait indefinitely) so the process sleeps.

### Expected effect

Cuts idle CPU and multiplies the benefit of every other optimization (filter
rebuild, cell parse, formatting) by reducing how often they run.

### Risk / notes

Keep immediate redraw after input so key repeat and mouse drag stay snappy.
Mouse drag can still set dirty every event; consider coalescing drag redraws to
one per poll interval if needed.

---

## P0 — Incremental filter (and sort) cache while scanning

### Problem

`matching_row_indices` treats any `cached_row_count != preview.row_count()` as
full invalidation. During background scan the row count grows every tick, so the
UI rebuilds the entire matching-row `Vec` (and re-sorts if sort is active) on
nearly every frame — even with **no** value filters (hidden-row checks still
scan `0..R`).

### Suggestion

1. **Append-only rebuild when only new rows arrived:**
   - Keep existing `cached_matching_rows` for `0..cached_row_count`.
   - Evaluate only `cached_row_count..current_count` and append survivors.
   - Same idea for `cached_sorted_scrollable_rows` when sort is inactive
     (append); when sort is active, either defer full re-sort (see next item)
     or merge new keys into a sorted structure.
2. **Batch invalidation:** only rebuild when row count advanced by N rows or
   after a time budget (e.g. 100–200ms), so title-bar row count can update more
   often than the full filter list.
3. **Fast path with no filters and no hidden rows:** store
   `cached_matching_rows = None` meaning “identity range”, and materialize
   `(0..count)` only when a caller needs a slice — or keep a generation flag
   `Identity` vs `Materialized` to avoid allocating a million indices.

### Expected effect

Removes the O(R) (or O(R×F) + sort) tax from every scan tick. Largest
responsiveness win for multi-hundred-thousand-row files with filters or sort.

### Risk / notes

Hidden-row and pin state must still be applied correctly for newly indexed rows.
Document the incremental contract next to the existing cache rules in
[row filtering](row-filtering.md).

---

## P0 — Sort: extract keys once, then sort indices

### Problem

`rebuild_sorted_scrollable_cache` uses `sort_by` where each comparison calls
`row_fields` **twice**, clones the sort-column cell, and runs type-aware
compare. For `R` rows that is roughly `O(R log R)` full-row CSV parses — the
worst interactive path in the codebase today.

### Suggestion

1. Precompute `Vec<(SortKey, usize)>` (or parallel arrays) with **one**
   `row_fields` / cell extract per row.
2. Sort the keys (stable or unstable as preferred).
3. Prefer parsing only the needed column when possible (see P1 column slice
   API) so sort does not decode unused fields.
4. While scanning with an active sort: either
   - show unsorted new rows below a “sorted so far” prefix until scan settles, or
   - re-sort on a background thread and swap the cache when ready (UI keeps
     previous order until then).

### Expected effect

Turns sort from “UI freeze” into a single linear pass plus an in-memory sort of
compact keys.

### Risk / notes

`SortKey` must own or intern text for string/date columns carefully to avoid
doubling peak memory. Cross-type fallback in `sort.rs` should use the same
precomputed keys.

---

## P1 — `row_fields`: less contention, less work per call

### Problem

Every `row_fields(i)` takes the exclusive `PreviewInner` mutex, runs a fresh
`csv::Reader` on the record slice, and allocates `Vec<String>`. Call sites
multiply this: viewport draw (~V cells), filter rebuild (R rows), sort
comparisons (R log R). Preview and layout both use `Mutex`, so the background
scan’s per-row locks contend with UI reads.

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

Work that unlocks the rest first:

1. **Dirty-flag + scan-throttled redraw** (P0) — small, localized to `tui/app.rs`.
2. **Identity / incremental matching-row cache** (P0) — `model.rs` + row-filter docs.
3. **Sort key precomputation** (P0) — `model.rs` / `sort.rs`.
4. **Parsed numeric filter AST + single-column field extract** (P1).
5. **RwLock + batched scan updates + viewport row cache** (P1).
6. **Per-frame kind snapshot + ASCII format fast path** (P2).
7. **Stats observe trimming** (P2).
8. **Web snapshot throttle / unlock** (P3).

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
