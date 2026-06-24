# TUI

Full-screen terminal table explorer. Stack: **ratatui** 0.29 + **crossterm**. Frontend: `csv-utils/src/tui/app.rs`.

## Screen layout

```
┌─ csv-utils │ file.csv │ N rows [loading…] ─────────────────────┐
│ ┌─ Data (rows A–B) ─────────────┐ ┌─ Columns (X–Y/Z) ────────┐ │
│ │ header + visible rows         │ │ idx: name                │ │
│ │ resizable cells, col scroll   │ │ independent list scroll  │ │
│ └───────────────────────────────┘ └──────────────────────────┘ │
│ q quit  ↑↓ rows  ←→ cols  drag resize  c info  ? help          │
└─────────────────────────────────────────────────────────────────┘
```

| Region | Description |
|--------|-------------|
| **Title** | File basename, live row count, `loading…` or `ERROR` |
| **Data table** | Horizontal window (`col_offset`) + vertical window (`row_offset`). Selected cell yellow; selected row dimmed |
| **Columns pane** | 32-char wide; title `Columns (X–Y/Z)`; selected line `▸` + magenta |
| **Help** | Centered overlay; `?` opens; `q` / `?` closes |
| **Column info** | Centered overlay; `c` opens; edit type/representation and view statistics; `q` closes |

Data table uses ratatui `Table`. Column sidebar uses manual `Paragraph` lines (not ratatui `List`; see [column list scrolling](#column-list-scrolling)).

## View state

`TableViewState` in `csv-utils-core/src/model.rs`:

| Field | Role |
|-------|------|
| `selected_row`, `selected_col` | Active cell (0-based). Row max = loaded body lines − 1 |
| `row_offset` | First body row in table viewport |
| `col_offset` | First column in table viewport |
| `column_list_offset` | First column shown in sidebar (independent of selection) |
| `column_widths` | Per-column cell width in characters (auto-fit 4–64; manual drag locks column) |
| `column_kinds` | Per-column type override (`Auto` = infer from loaded rows) |
| `column_numeric_repr` | General vs scientific formatting for numeric columns |
| `column_widths_user_set` | Manual resize lock per column |
| `show_column_info` | Column info overlay visible |
| `column_info_focus` | Highlighted option in info panel (type options, then representation) |
| `show_help` | Help overlay visible |

Each frame: `maybe_refit_column_widths()` (when loaded row count changes), `clamp_selection(viewport_rows, table_width)`, and `clamp_column_list_offset(visible_height)`.

## Keyboard

| Key | Action |
|-----|--------|
| `q` | Quit; closes an open panel when one is visible |
| `↑`/`↓` or `j`/`k` | Previous / next row |
| `←`/`→` or `h`/`l` | Previous / next column |
| `PgUp`/`PgDn` | Move selection ±10 rows |
| `Home`/`End` | First / last loaded row |
| `c` | Open column info panel |
| `?` | Help overlay |

### Column info (`c`)

While the panel is open, table navigation is disabled:

| Key | Action |
|-----|--------|
| `↑`/`↓` or `j`/`k` | Move highlight between type/representation options |
| `Enter` | Apply highlighted option |
| `q` | Close panel |

The panel shows editable **type** options filtered by inferred data (e.g. text-only columns hide date/int/float), **representation** when numeric types apply, plus type-specific **statistics** from loaded rows (note shown while scanning).

## Mouse

| Target | Action |
|--------|--------|
| Column info panel | Click type/representation rows to apply |
| Table header border | Drag to resize column width (4–64 chars) |
| Table header | Select column only (click, not on border) |
| Table body cell | Select row + column |
| Table wheel | Move `selected_row` ±3 |
| Column list click | Select column |
| Column list wheel | Scroll sidebar ±3 via `column_list_offset` |

Hit-testing: `hit_test_table` / `hit_test_column_resize` in `app.rs` (variable-width columns + 1-char spacing).

## Column list scrolling

Sidebar uses `column_list_offset` independent of selection. ratatui `List` was avoided because it resets offset each frame to keep the selected item visible, which blocked wheel scrolling past the current selection.

- Scroll max: `headers.len() − visible_height`
- Wheel updates offset only
- Selection changes call `ensure_column_list_shows_selection`

## Run

```bash
pixi run tui test-data/generated/test_1000x100.csv
./target/release/csv-utils tui file.csv
```

Press `?` in the TUI for inline help.

## Related

- [Data loading](../reference/data-loading.md)
- [CSV parsing & column types](../reference/csv-parsing.md)
- [Web UI](web.md) — parallel behavior in the browser
