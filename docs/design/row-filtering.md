# Design: row filtering

Status: **implemented** in `csv-utils-core` (TUI path).

Row filters let the user narrow visible table rows to those where a selected
column's value satisfies an expression. This doc covers the evaluation model,
caching strategy, and the performance constraints that motivated the design.

## Overview

| Concern | Mechanism |
|---------|-----------|
| Filter storage | `TableViewState::column_value_filters: Vec<Option<String>>` — one slot per column |
| Filter evaluation | `AppModel::row_passes_value_filters(row)` — interprets stored expressions |
| Filtered row list | `AppModel::matching_row_indices(&mut self) -> &[usize]` — cached; at most one scan per tick |
| Invalidation | Any `set_column_value_filter` / `clear_column_value_filter` call, or new rows arriving from the background scan |
| Read-only access in draw | `AppModel::cached_matching_rows(&self) -> Option<&[usize]>` |

## Filter expression types

### Text columns — fuzzy match

`column_value_filter::text_cell_matches(cell, query)` calls `fuzzy_score(query, cell)`.
A non-empty query matches a cell if it is a subsequence of the cell value (case-insensitive).

### Numeric columns — expression parser

`column_value_filter::numeric_cell_matches(cell, expr)` parses `expr` with a
recursive-descent parser before evaluating it against the cell's parsed `f64`.

Grammar (informal):

```
expr   = or
or     = and ('|' and)*
and    = primary ('&' primary)*
primary = '(' expr ')' | compare
compare = op number
op     = '>=' | '<=' | '==' | '!=' | '>' | '<'
number = ['-'] digit+ ['.' digit+]
```

Examples: `>10`, `(>=10) & (< 20)`, `(==0) | (==1)`, `!= -1.5`

`validate_numeric_filter(expr)` parses without evaluating; used before storing a
filter so invalid expressions are rejected at entry time (the command line and
column info panel both call this).

Location: `csv-utils-core/src/column_value_filter.rs`

## Fuzzy scoring

`fuzzy_score(query, target) -> Option<u32>` is a subsequence-based scorer:

- Returns `None` if `query` is not a subsequence of `target`.
- Returns `Some(score)` otherwise; higher = better match.
- Bonus for consecutive matched characters (`run` streak), word-boundary
  positions (character after a non-alphanumeric), and first-character matches.

`rank_by_fuzzy(query, items)` sorts a list of `(index, name)` pairs by
descending score, breaking ties by original index.

Location: `csv-utils-core/src/fuzzy.rs`

## Performance design

### The problem

`matching_row_indices` must produce an ordered list of row indices that pass all
active column filters. It is consumed in multiple places per rendered frame:

| Call site | Why it needs filtered rows |
|-----------|---------------------------|
| `draw_table` (body rows) | Slice by `row_offset..row_offset+height` |
| `draw` (title bar) | `visible / total` row count |
| `hit_test_table` | Map mouse Y → actual row index |
| `clamp_selection` | Keep `selected_row` and `row_offset` within visible rows |
| `snap_selection_to_visible_rows` | After a filter changes |
| `move_selected_row` (navigation) | Step only through visible rows |
| `Home` / `End` keys | First / last visible row |

Without caching, each call scans every indexed row and evaluates all active
filter expressions — O(R × F) per call, multiplied by the number of call sites
per frame. On a 100 k-row file with even a single active filter this made the
UI completely unresponsive.

### Solution: write-invalidated cache

`TableViewState` holds:

```rust
cached_matching_rows: Option<Vec<usize>>,  // None = stale
cached_row_count: usize,                   // row count at last build
```

`matching_row_indices(&mut self) -> &[usize]`:

1. If `cached_matching_rows` is `None` (or `cached_row_count > row_count`): full
   O(R × F) scan over `0..row_count`, then rebuild the sorted scrollable cache.
2. Else if `cached_row_count < row_count` (background scan appended rows):
   evaluate only `cached_row_count..row_count`, **append** survivors, and either
   append to the sorted scrollable cache (no sort) or rebuild it with
   precomputed sort keys (sort active).
3. Else: cache hit — return the existing slice.

Invalidation points (set `cached_matching_rows = None`, reset `cached_row_count`):

| Trigger | Code |
|---------|------|
| Filter applied | `set_column_value_filter` |
| Filter cleared | `clear_column_value_filter` |
| Rows hidden / unhidden | `hide_selected_rows` / `unhide_…` |
| New rows from background scan | Incremental append (not a full invalidate) |

The cache is warmed when the TUI draws (`maybe_update_column_layout` before
`terminal.draw`). The TUI only draws on input, resize, or throttled scan
progress — see [Performance & TUI responsiveness](performance-tui-responsiveness.md).
All draw functions use the read-only `cached_matching_rows(&self)` accessor;
they never trigger a rescan.

### Cost profile

| Scenario | Scan cost |
|----------|-----------|
| No filter active, cache valid | O(1) — return existing slice |
| Filter active, no new rows | O(1) — cache hit |
| N new rows since last access (no filter change) | O(N × F) append; sort rebuild O(R) keys if sort active |
| Filter / hide changed | O(R × F) once on the next access |

### Borrow-checker considerations

Because `matching_row_indices` takes `&mut self` (to write the cache), callers
that also need to write `self.view` (e.g. `move_selected_row`, `clamp_selection`,
`snap_selection_to_visible_rows`) cannot hold the returned `&[usize]` reference
across a mutable access.  The pattern used is:

```rust
self.matching_row_indices();             // warm the cache
let sel = self.view.selected_row;        // copy the value we need
let m = self.view.cached_matching_rows   // borrow the Vec directly
            .as_deref().unwrap();
let target = m[...];                     // use it
drop(m);                                 // end the borrow
self.view.selected_row = target;         // now safe to mutate
```

This avoids both redundant re-scans and borrow conflicts.

## UI indicators

| Indicator | Location | Condition |
|-----------|----------|-----------|
| `*` suffix on column name | Table header + sidebar label | `column_has_value_filter(col)` |
| `visible/total rows` | Title bar | `row_value_filters_active()` |
| Filter expression line | Column info panel (`c`) | Always shown; editable |

## Column name filter (sidebar)

The column finder (`/` key, `ColumnFinderState`) applies a **separate** fuzzy
filter on column *names* (`column_name_filter` in `TableViewState`). This does
not affect `matching_row_indices`; it only controls which columns appear in the
sidebar and is evaluated in `filtered_sidebar_columns()` on every draw (the
column count is at most a few hundred, so no caching is needed).

## Related

- [Architecture](../architecture.md) — row-filter cache in the view model overview
- [Data loading](../reference/data-loading.md) — background scan and row count growth
- [Performance & TUI responsiveness](performance-tui-responsiveness.md) — incremental cache and filter eval proposals
- [TUI](../features/tui.md) — keyboard and column info panel usage
