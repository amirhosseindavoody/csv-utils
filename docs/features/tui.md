# TUI

Full-screen terminal table explorer. Stack: **ratatui** 0.29 + **crossterm**. Frontend: `csv-utils/src/tui/app.rs`.

## Screen layout

```
┌─ csv │ file.csv │ N rows [loading…] ─────────────────────┐
│ ┌───────────────────────────────┐ ┌─ Columns (X–Y/Z) ────────┐ │
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
| `column_info_focus` | Highlighted option in info panel (type, representation, decimal places) |
| `column_info_decimal_editing` | TUI: editing decimal format text |
| `show_help` | Help overlay visible |
| `show_column_borders` | Draw column `│` lines and a header `─` rule in the table gaps (initialized from config; toggled with `:toggle-borders`). Gaps stay as whitespace when off |
| `column_name_filter` | Fuzzy name filter for the column sidebar (`/` finder) |
| `column_value_filters` | Per-column row value filters (`:filter` on selected column) |
| `column_sidebar_focused` | When true, `:filter` applies to the sidebar instead of row values |
| `column_hidden` | Per-column flag: hidden from the table but still listed in the sidebar |
| `multi_selected_cols` | Ctrl+click multi-selection for bulk `:hide` (empty = use `selected_col` only) |
| `row_hidden` | Per-row flag: hidden from the table (session-only) |
| `multi_selected_rows` | Ctrl+click on table body for bulk row `:hide` (empty = use `selected_row` only) |

Settings load from `csv-utils.json` in the working directory on open; see [settings config](../design/settings-config.md).

Each frame: `maybe_refit_column_widths()` (when loaded row count changes), `clamp_selection(viewport_rows, table_width)`, and `clamp_column_list_offset(visible_height)`.

## Keyboard

| Key | Action |
|-----|--------|
| `q` | Quit; closes an open panel when one is visible |
| `↑`/`↓` or `j`/`k` | Previous / next row |
| `←`/`→` or `h`/`l` | Previous / next column |
| `Space` | Toggle multi-select on the current row or column (follows the last arrow axis) |
| `PgUp`/`PgDn` | Move selection ±10 rows |
| `Home`/`End` | First / last loaded row |
| `c` | Open column info panel |
| `?` | Help overlay |
| `:` | Open command line (filtered suggestions, Tab complete) |
| `/` | Open column finder (fuzzy-match column names; filters sidebar live) |
| `:` then `:open <path>` | Open another file, or browse a directory in the file picker |
| `:` then `:close` | Close file and return to file picker (in last file's directory) |
| `:` then `:toggle-borders` | Show or hide `│` border lines between table columns for this session |
| `:` then `:hide` / `:h` | Hide selected column(s) when the **sidebar** is focused, or selected row(s) when the **table** is focused |
| `:` then `:filter <text>` / `:f <text>` | Filter **rows** on the selected column (text: fuzzy; numeric: `>10`, `(>=10) & (<20)`, etc.) |
| `:` then `:filter` / `:f` | Clear row filter on the selected column |
| Sidebar focused + `:filter <text>` | Filter the column **sidebar** by name (click or scroll sidebar to focus) |

Command line keys: **↑/↓** select suggestion, **Tab** complete, **Enter** run (for `:open` and `:filter`, Enter selects the command first, then type the argument and press **Enter** again), **Esc** cancel.

Column finder keys (**`/`**): type to fuzzy-filter the sidebar, **↑/↓** pick a match, **Enter** jump to that column (filter stays active), **Esc** cancel and clear the filter.

Filtered columns show `*` in the table header and column sidebar. The title bar shows `visible/total rows` when any row filter is active. Edit or clear filters in the column info panel (**c** → **Row filter**).

### File picker (no file on launch)

Shown when `csv` or `csv tui` is run without a path. By default only files
matching `file_picker.file_extensions` in `csv-utils.json` (`.csv`, `.dat`) are
listed; directories are always shown.

| Key | Action |
|-----|--------|
| `↑`/`↓` or `j`/`k` | Previous / next entry |
| `PgUp`/`PgDn` | Move selection by one page |
| `→` | Enter selected directory or open file |
| `←` | Parent directory |
| `Enter` | Enter directory or open file |
| `:` then `:open <path>` | Open file by relative or absolute path |
| `:` then `:all` / `:a` | Show all files |
| `:` then `:filter` / `:f` | Restore extension filter |
| `q` / `Esc` | Quit (Esc cancels a command) |
| Click | Select entry (same as `Enter`) |

Command line keys: **↑/↓** select suggestion, **Tab** complete, **Enter** run (for `:open`, Enter selects the command first, then type/paste the path and press **Enter** again), **Esc** cancel.

### Column info (`c`)

While the panel is open, table navigation is disabled:

| Key | Action |
|-----|--------|
| `↑`/`↓` or `j`/`k` | Move highlight between type, representation, and decimal places |
| `Enter` | Apply highlighted option; on **Decimal places** or **Row filter**, start edit or apply typed value |
| Type directly | When decimal row is focused, type a format (e.g. `.5`) |
| `Backspace` | While editing decimal format, delete a character |
| `q` | Close panel |

The panel shows editable **type** options filtered by inferred data (e.g. text-only columns hide date/int/float), **representation** when numeric types apply, **decimal places** (text field, default `.3` from `csv-utils.json`), **row filter** (fuzzy text or numeric expression), plus type-specific **statistics** from loaded rows (note shown while scanning).

## Mouse

| Target | Action |
|--------|--------|
| Column info panel | Click type/representation rows to apply; click decimal field to focus |
| Table header border | Drag to resize column width (4–64 chars) |
| Table header | Select column only (click, not on border); **Ctrl+click** toggles column multi-select |
| Table body cell | Select row + column; **Ctrl+click** toggles row multi-select |
| Table wheel | Move `selected_row` ±3 |
| Column list click | Select column; **Ctrl+click** toggles column multi-select |
| Column list wheel | Scroll sidebar ±3 via `column_list_offset` |

Multi-selected columns show a blue highlight (`◆` prefix in the sidebar). Multi-selected rows use a blue row background; the active row keeps the primary highlight (yellow cell, dark gray row). Hidden columns remain in the sidebar with a dim `·` prefix but are omitted from the table. Hidden rows are omitted from the table entirely. At least one column and one row must stay visible; `:hide` reports an error if the selection would hide every column or every row.

Hit-testing: `hit_test_table` / `hit_test_column_resize` in `app.rs` (variable-width columns plus a one-character gap between columns; gap shows `│` when column borders are enabled).

## Column list scrolling

Sidebar uses `column_list_offset` independent of selection. ratatui `List` was avoided because it resets offset each frame to keep the selected item visible, which blocked wheel scrolling past the current selection.

- Scroll max: `headers.len() − visible_height`
- Wheel updates offset only
- Selection changes call `ensure_column_list_shows_selection`

## Run

```bash
pixi run tui
pixi run tui test-data/generated/test_1000x100.csv
./target/release/csv
./target/release/csv tui file.csv
```

With no file argument, the TUI opens a **file picker** starting in the current working directory. Only files with extensions from `csv-utils.json` (`file_picker.file_extensions`, default `.csv` and `.dat`) are listed; type `:all` to show every file. Navigate with `→` / `←` for directories, select a file with `Enter`, or quit with `q`.

Press `?` in the TUI for inline help.

## Related

- [Data loading](../reference/data-loading.md)
- [CSV parsing & column types](../reference/csv-parsing.md)
- [Web UI](web.md) — parallel behavior in the browser
