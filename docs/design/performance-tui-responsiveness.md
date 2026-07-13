# Performance & TUI responsiveness

How the interactive path stays usable while files are scanning, filtering, or sorting. Builds on [large-file preview](large-file-preview.md), [row filtering](row-filtering.md), and [data loading](../reference/data-loading.md).

## Shipped behavior

| Mechanism | Effect |
|-----------|--------|
| mmap + offset index + on-demand parse | Fast first paint; RSS tracks touched pages |
| Background scan after the first ~128 rows | Navigation can start early |
| Dirty-flag TUI redraw | Draw on input, resize, or throttled scan progress (~100ms); idle CPU stays low |
| Incremental matching-row cache | New scan rows append in O(Δ); full rebuild only when filters/hides invalidate |
| Sorted-row cache + precomputed sort keys | One `row_fields` parse per row, then an in-memory key sort |
| Detached drop of old `AppModel` on close | Closing a file does not block on freeing large stats |
| Web `ClientView` behind `RwLock` | `GET /api/state` does not hold the model mutex |

### Event loop

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

### Background scan (per indexed row)

```text
lock PreviewInner → push offset
lock ColumnLayoutState → observe_fields (width + infer + stats)
```

## Remaining opportunities

Ordered by how often cost hits the UI thread and how it scales with rows `R`, columns `C`, viewport `V`, and filters `F`.

### Cheaper row access and less lock contention

Every `row_fields(i)` takes the preview mutex, runs a fresh CSV parse on the record slice, and allocates. Draw, filter rebuild, and sort all call it.

Possible improvements: `RwLock` for shared reads during scan; batch offset/layout updates from the scan thread; a small viewport row cache; single-column extract for filter/sort; `Arc` headers instead of cloning.

### Filter evaluation

Numeric filter strings are re-parsed per row on each rebuild; fuzzy scoring allocates per cell; `row_passes_value_filters` parses full rows even when one column is filtered.

Possible improvements: store a parsed expression AST next to each numeric filter; ASCII fast paths for fuzzy compare; single-column extract inside the filter pass.

### Draw-path allocations

Per visible cell, draw may lock layout for type resolution and allocate short-lived strings. Index vectors are often cloned.

Possible improvements: snapshot column kinds once per frame; ASCII-only format fast path; reuse buffers for truncated display strings.

### Stats observe cost

Background scan updates width/inference/stats under a lock for every row.

Possible improvements: sample or throttle distinct-value tracking; batch observe calls with offset appends.

### Web snapshot cadence

While scanning after `:web`, a background task rebuilds `ClientView` on an interval under the model lock.

Possible improvements: same dirty/throttle rules as the TUI; build cell grids outside the hot lock when only metadata changed.

## Measuring

Useful scenarios (release builds via Pixi):

| Scenario | Metric |
|----------|--------|
| Idle, scan done | CPU % / draws per second |
| Scan ~1M×20, no filter | Time in `matching_row_indices` per tick |
| Same with one text filter | Rebuild time; input latency |
| `:sort` on ~100k rows | Wall time of sorted-cache rebuild |
| Scroll a 30×15 viewport | Time in `draw_table` / `row_fields` |

## Related

- [Architecture](../architecture.md)
- [Data loading](../reference/data-loading.md)
- [Large-file preview](large-file-preview.md)
- [Row filtering](row-filtering.md)
- [Known limitations](../reference/limitations.md)
