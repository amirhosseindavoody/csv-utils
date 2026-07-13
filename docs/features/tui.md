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
| **Data table** | Horizontal window (`col_offset`) + vertical window (`row_offset`). Selected column dim gray stripe (full height); selected row dimmed; active cell yellow |
| **Columns pane** | Resizable width (drag left border, 16–80 chars); title `Columns (X–Y/Z)`; selected line `▸` + magenta |
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
| `show_row_json` | Row JSON floating panel / fullscreen view visible |
| `row_json_fullscreen` | Borderless full-terminal Row JSON text view (`f` toggle; does not change panel geometry) |
| `show_column_borders` | Draw column `│` lines and a header `─` rule in the table gaps (initialized from config; toggled with `:toggle-borders`). Gaps stay as whitespace when off |
| `column_name_filter` | Fuzzy name filter for the column sidebar (`/` finder) |
| `column_value_filters` | Per-column row value filters (`:filter` on selected column) |
| `column_sidebar_focused` | When true, `:filter` applies to the sidebar instead of row values; **↑/↓** navigate columns |
| `column_sidebar_width` | Column sidebar pane width in terminal columns (default 32; drag left border to resize) |
| `column_status` | Per-column table status: `normal` (default), `pinned`, or `hidden` (mutually exclusive; hidden columns stay in the sidebar) |
| `column_pin_order` | Chronological list of pinned column indices (left-to-right in the table and top of the sidebar; horizontal scroll applies only to normal columns) |
| `multi_selected_cols` | Ctrl+click multi-selection for bulk `:hide` (empty = use `selected_col` only); cleared when row **Space** multi-select or cell range starts |
| `row_status` | Per-row table status: `normal`, `pinned`, or `hidden` (mutually exclusive; session-only) |
| `row_pin_order` | Chronological list of pinned row indices (fixed at the top of the table; vertical scroll applies only to normal rows) |
| `multi_selected_rows` | **Space** toggles individual rows for bulk row `:hide`; cleared when column multi-select or cell range starts |
| `cell_range_anchor` / `cell_range_focus` | Inclusive corners for click-drag rectangular cell selection; `:hide` uses the row span |
| `multi_selected_cells` | Individual `(row, col)` pairs toggled with Ctrl+click; `:hide` uses their distinct rows |
| `sort_column` / `sort_direction` | Session-only row sort on one column (`asc` or `desc`); pinned rows stay above the sorted block |

Settings load from `~/.config/csv-utils/csv-utils.json` (created on first open), with optional `./csv-utils.json` in the working directory overriding individual fields; see [settings config](../design/settings-config.md).

Each frame (when the TUI redraws): `maybe_refit_column_widths()` (when loaded row count changes), `clamp_selection(viewport_rows, table_width)`, and `clamp_column_list_offset(visible_height)`. The event loop draws only when dirty — after input, resize, or throttled background-scan progress (~100ms) — so idle CPU stays low once loading finishes. Dragging the table row/column scrollbar decouples scroll from the selected cell until you move selection (arrows, click, or wheel on the table). Scrollbar thumb size and drag mapping use a stable viewport length (rows: scrollable rows that fit; columns: fit-from-start count matching `max_col_offset`) so the thumb does not jitter while dragging, and the thumb sits flush with the track end at max scroll.

## Keyboard

| Key | Action |
|-----|--------|
| `q` | Close open panel; with a file loaded, return to file picker; from file picker, quit |
| `↑`/`↓` or `j`/`k` | Previous / next row (pinned rows first in pin order, then scrollable rows); when the **sidebar is focused** (click or scroll it), previous / next column |
| `←`/`→` or `h`/`l` | Previous / next **visible** column (pinned columns first in pin order, then scrollable columns; hidden columns are skipped) |
| `Space` | Toggle multi-select on the current row or column (follows the last arrow axis) |
| `PgUp`/`PgDn` | Move selection ±10 rows |
| `Home`/`End` | First / last row in navigation order (pinned rows first, then scrollable) |
| `c` | Open column info panel |
| `r` | Open floating panel with the selected row as pretty-printed JSON (syntax-highlighted) |
| `p` | Pin or unpin selected row(s) after **↑/↓** (row axis), or column(s) when the sidebar is focused or the last arrow axis was column (**←/→**); works with multi-select |
| `?` | Help overlay |
| `:` | Open command line (filtered suggestions, Tab complete) |
| `/` | Open column finder (fuzzy-match column names; filters sidebar live) |
| `:` then `:open <path>` | Open another file, or browse a directory in the file picker |
| `:` then `:close` | Close file and return to file picker (in last file's directory) |
| `:` then `:toggle-borders` | Show or hide `│` border lines between table columns for this session |
| `:` then `:hide` / `:h` | Hide selected column(s) after **←/→** or sidebar focus, or selected row(s) after **↑/↓**; Ctrl+click column header/sidebar for column multi-select |
| `:` then `:unhide` / `:u` | Unhide using the same row/column axis as `:hide`; with no hidden targets in the selection, unhide **all** hidden rows or columns for that axis |
| `:` then `:sort` | Sort rows by the selected column (ascending → descending → clear); `:sort asc`, `:sort desc`, or `:sort clear` for explicit control |
| `:` then `:web` | Open browser UI on a free local port and exit the terminal view (Ctrl+C stops the server) |
| `:` then `:filter <text>` / `:f <text>` | Filter **rows** on the selected column (text: fuzzy; numeric: `>10`, `(>=10) & (<20)`, etc.) |
| `:` then `:filter` / `:f` | Clear row filter on the selected column |
| Sidebar focused + `:filter <text>` | Filter the column **sidebar** by name (click or scroll sidebar to focus) |

Command line keys: **↑/↓** select suggestion, **Tab** complete, **Enter** run (for `:open` and `:filter`, Enter selects the command first, then type the argument and press **Enter** again), **Esc** cancel.

Column finder keys (**`/`**): type to fuzzy-filter the sidebar, **↑/↓** pick a match, **Enter** jump to that column (filter stays active), **Esc** cancel and clear the filter.

Filtered columns show `*` in the table header and column sidebar. Sorted columns show `↑` (ascending) or `↓` (descending) in the header. The title bar shows `visible/total rows` when any row filter is active. Edit or clear filters in the column info panel (**c** → **Row filter**).

### File picker (no file on launch)

Shown when `csv` or `csv tui` is run without a path. By default **all** files and
directories are listed. To filter by extension, add `file_picker.file_extensions` to
settings (global or local `csv-utils.json`); then **`:filter`** / **`:f`** applies
that filter and **`:all`** / **`:a`** shows every file again.

| Key | Action |
|-----|--------|
| `↑`/`↓` or `j`/`k` | Previous / next entry |
| `/` | Fuzzy-filter files and folders by name (live; **Esc** clears) |
| `PgUp`/`PgDn` | Move selection by one page |
| `→` | Enter selected directory or open file |
| `←` | Parent directory (highlights the directory you came from) |
| `Enter` | Enter directory or open file |
| `:` then `:open <path>` | Open file by relative or absolute path |
| `:` then `:all` / `:a` | Show all files (when an extension filter is active) |
| `:` then `:filter` / `:f` | Apply extension filter from settings (when `file_picker.file_extensions` is configured) |
| `q` / `Esc` | Quit (Esc cancels a command) |
| Click | Select entry (same as `Enter`) |

Command line keys: **↑/↓** select suggestion, **Tab** complete, **Enter** run (for `:open`, Enter selects the command first, then type/paste the path and press **Enter** again), **Esc** cancel.

### Column info (`c`)

While the panel is open, table navigation is disabled:

| Key | Action |
|-----|--------|
| `↑`/`↓` or `j`/`k` | Move highlight between type, representation, and decimal places |
| `PgUp`/`PgDn` | Scroll panel when statistics extend past the viewport |
| `Enter` | Apply highlighted option; on **Decimal places** or **Row filter**, start edit or apply typed value |
| Type directly | When decimal row is focused, type a format (e.g. `.5`) |
| `Backspace` | While editing decimal format, delete a character |
| `q` | Close panel |

The panel shows editable **type** options filtered by inferred data (e.g. text-only columns hide date/int/float), **representation** when numeric types apply, **decimal places** (text field, default from merged settings, e.g. `.3`), **row filter** (fuzzy text or numeric expression), plus type-specific **statistics** from loaded rows (note shown while scanning; stats accumulate during background load). A vertical scrollbar appears when content exceeds the viewport.

### Row JSON (`r`)

Shows the currently selected row as pretty-printed JSON (`header → value` object) with syntax highlighting (keys cyan, strings green, numbers yellow, literals magenta). While open, table navigation is disabled:

| Key | Action |
|-----|--------|
| `↑`/`↓` or `j`/`k` | Scroll vertically |
| `←`/`→` or `h`/`l` | Scroll horizontally |
| `PgUp`/`PgDn` | Page vertically |
| `Home`/`End` | Jump to start / end of content |
| `f` | Toggle borderless full-screen text view (like `less`/`bat`; does **not** resize the floating panel) |
| `q` or `r` | Close panel |

Mouse:

| Target | Action |
|--------|--------|
| Title bar | Drag to move the floating panel (panel mode only) |
| Bottom-right corner | Drag to resize (min 30×8 cells; panel mode only) |
| Vertical / horizontal scrollbars | Drag thumb/track or use wheel (panel mode only) |
| Panel / fullscreen body wheel | Scroll vertically |

**Fullscreen (`f`):** replaces the UI with a borderless full-terminal text view and a single dim status line at the bottom (`f leave fullscreen · q close`). Floating panel position and size are preserved and restored when leaving fullscreen. Drag, resize, and scrollbars are disabled while fullscreen.

Opening the row JSON panel closes column info (and vice versa). Panel position and size persist for the session after the first drag or resize.

## Mouse

| Target | Action |
|--------|--------|
| Column info panel | Click type/representation rows to apply; click decimal field to focus; **PgUp/PgDn** or mouse wheel scroll |
| Row JSON panel | Drag title to move; drag bottom-right corner to resize; wheel / scrollbars for overflow; `f` toggles borderless fullscreen (panel geometry unchanged) |
| Table header border | Drag to resize column width (4–64 chars) |
| Table header | Select column only (click, not on border); **Ctrl+click** adds column to selection; **right-click** opens column context menu |
| Table body cell | Click to select; **drag** to select a rectangular cell range; **Ctrl+click** toggles individual cells (no fill between) |
| Row gutter (`▸`/`▐`) **right-click** | Open context menu: **Select**, **Hide** / **Unhide**, **Pin** / **Unpin** |
| Table wheel | Move `selected_row` ±3 |
| Table / column sidebar / column info | Scrollbars (▲▼ / ◀▶) when content exceeds the viewport; drag thumb or track (thumb reaches the track end at max scroll); wheel over a scrollbar scrolls that pane |
| Column list click | Select column; **Ctrl+click** adds column to selection |
| Column list **right-click** | Open context menu: **Select**, **Hide** / **Unhide**, **Info**, **Pin** / **Unpin**, **Sort ascending** / **Sort descending**, **Clear sort** |
| Column list left border | Drag to resize sidebar width (16–80 chars) |
| Column list wheel | Scroll sidebar ±3 via `column_list_offset` |

Multi-selected columns show a blue highlight down the full column (`◆` prefix in the sidebar). Multi-selected rows use a blue row background. **Drag** on table body cells highlights a blue rectangle; **Ctrl+click** on cells toggles individual blue cells (without filling the area between them). With only the cursor on a cell (no row/column/cell multi-select), the **column header** and a **`▸` row gutter** mark the current row/column — body cells are not striped. The active cell keeps the yellow highlight. Row/column multi-select (**Space** or Ctrl+click on headers/sidebar/gutter) and cell selection (drag or Ctrl+click) are mutually exclusive — starting one clears the others. Arrow keys and plain clicks move focus without clearing the other selection mode. Hidden columns remain in the sidebar with a dim `·` prefix at the **end** of the list (after pinned and visible unpinned columns) but are omitted from the table and skipped by `←`/`→`. Pinned columns show a cyan `▐` prefix in the sidebar, are listed first in chronological pin order (matching their fixed left position in the table), and stay visible while unpinned columns scroll horizontally. Pinned rows show a cyan `▐` in the row gutter, stay fixed at the top of the table in chronological pin order, and unpinned rows scroll vertically beneath them. Select a hidden column in the sidebar and run `:unhide` to show it again; run `:unhide` on the table to restore hidden rows. Hidden rows are omitted from the table entirely. At least one column and one row must stay visible; `:hide` reports an error if the selection would hide every column or every row. With an active cell range or Ctrl+selected cells, `:hide` on the table hides every row spanned by the selection.

Context menu keys (after right-click on the column sidebar, table header, or row gutter): **↑/↓** move highlight, **Enter** activate, **Esc** / **q** dismiss; left-click an item to activate. **Ctrl+right-click** adds the column or row to a multi-selection (same as Ctrl+click). **Select** selects the clicked column or row on the first use; on later uses it adds to the existing selection (seeding the previously-selected column/row when building the first multi-select). Other menu actions preserve an existing multi-select and include the clicked column or row in bulk operations. Plain click (no Ctrl) on the sidebar, table header, or row gutter still replaces the selection with a single column or row.

Hit-testing: `hit_test_table` / `hit_test_column_resize` in `app.rs` (variable-width columns plus a one-character gap between columns; gap shows `│` when column borders are enabled).

## Column list scrolling

Sidebar uses `column_list_offset` independent of selection. ratatui `List` was avoided because it resets offset each frame to keep the selected item visible, which blocked wheel scrolling past the current selection.

- Scroll max: `headers.len() − visible_height`
- Wheel updates offset only
- Selection changes call `ensure_column_list_shows_selection`

## Run

```bash
pixi run csv
pixi run csv test-data/generated/test_1000x100.csv
./target/release/csv
./target/release/csv test-data/generated/test_1000x100.csv
./target/release/csv tui file.csv   # same as above
```

With no file argument, the TUI opens a **file picker** starting in the current working directory. All files and directories are shown by default. Configure `file_picker.file_extensions` in settings to enable extension filtering (`:filter` / `:all`). Navigate with `→` / `←` for directories, select a file with `Enter`, or quit with `q`.

Press `?` in the TUI for inline help.

## Related

- [Data loading](../reference/data-loading.md)
- [CSV parsing & column types](../reference/csv-parsing.md)
- [Web UI](web.md) — parallel behavior in the browser
