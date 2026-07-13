# TUI

Full-screen terminal table explorer (`csv-utils/src/tui/app.rs`, ratatui + crossterm).

## Screen layout

```
┌─ csv │ file.csv │ N rows [loading…] ─────────────────────────┐
│ ┌───────────────────────────────┐ ┌─ Columns (X–Y/Z) ──────┐ │
│ │ header + visible rows         │ │ name list              │ │
│ │ scroll, resize, pin/hide      │ │ independent scroll     │ │
│ └───────────────────────────────┘ └────────────────────────┘ │
│ q quit  ↑↓ rows  ←→ cols  c info  r JSON  ? help             │
└──────────────────────────────────────────────────────────────┘
```

| Region | Description |
|--------|-------------|
| **Title** | File basename, live row count, `loading…` / `ERROR`, and `visible/total` when filters or hidden rows apply |
| **Data table** | Scrollable rows and columns; yellow active cell; pinned rows/columns stay fixed |
| **Columns pane** | Sidebar of column names (drag left border to resize, 16–80 chars) |
| **Help** | Centered overlay (`?`) |
| **Column info** | Centered overlay (`c`) for type, format, filter, and stats |
| **Row JSON** | Floating panel (`r`) with the selected row as pretty-printed JSON |

Settings load from `~/.config/csv-utils/csv-utils.json`, with optional `./csv-utils.json` overrides. See [settings config](../design/settings-config.md).

The event loop redraws only when dirty (input, resize, or throttled scan progress), so idle CPU stays low after loading finishes.

## Keyboard

| Key | Action |
|-----|--------|
| `q` | Close open panel; with a file loaded, return to the file picker; from the picker, quit |
| `↑`/`↓` or `j`/`k` | Previous / next row (or column when the sidebar is focused) |
| `←`/`→` or `h`/`l` | Previous / next visible column |
| `Space` | Toggle multi-select on the current row or column (follows the last arrow axis) |
| `PgUp`/`PgDn` | Move selection ±10 rows |
| `Home`/`End` | First / last row in navigation order |
| `c` | Column info panel |
| `r` | Row as JSON (floating panel) |
| `p` | Pin/unpin selected row(s) or column(s) (follows row vs column axis) |
| `?` | Help |
| `:` | Command line |
| `/` | Fuzzy column finder (filters the sidebar live) |

### Commands (`:`)

| Command | Action |
|---------|--------|
| `:open <path>` | Open a file or browse a directory |
| `:close` | Close the file and return to the file picker |
| `:toggle-borders` | Show or hide `│` between table columns |
| `:hide` / `:h` | Hide selected columns or rows (axis follows focus / last arrows) |
| `:unhide` / `:u` | Unhide selection, or all hidden on that axis if none are selected-hidden |
| `:sort` | Cycle sort on the selected column (asc → desc → clear) |
| `:sort asc\|desc\|clear` | Set or clear sort explicitly |
| `:filter <text>` / `:f <text>` | Filter rows on the selected column, or the sidebar when it is focused |
| `:filter` / `:f` | Clear the active filter |
| `:web` | Open the browser UI and exit the terminal view |

Command line: **↑/↓** pick a suggestion, **Tab** complete, **Enter** run (for `:open` / `:filter`, Enter selects the command first, then type the argument), **Esc** cancel.

Column finder (`/`): type to filter, **↑/↓** pick a match, **Enter** jump (filter stays), **Esc** cancel and clear.

Filtered columns show `*` in the header and sidebar. Sorted columns show `↑` / `↓`. Edit or clear row filters from column info (**c** → **Row filter**).

### File picker

Shown when `csv` starts without a path. All files and directories are listed by default. Set `file_picker.file_extensions` in settings to enable extension filtering (`:filter` / `:all`).

| Key | Action |
|-----|--------|
| `↑`/`↓` or `j`/`k` | Move selection |
| `/` | Fuzzy name filter (**Esc** clears) |
| `PgUp`/`PgDn` | Page |
| `→` / `Enter` | Enter directory or open file |
| `←` | Parent directory |
| `:open <path>` | Open by path |
| `:all` / `:a` | Show all files (when an extension filter is active) |
| `:filter` / `:f` | Apply extension filter from settings |
| `q` / `Esc` | Quit (Esc also cancels a command) |
| Click | Same as Enter |

### Column info (`c`)

While open, table navigation is disabled.

| Key | Action |
|-----|--------|
| `↑`/`↓` or `j`/`k` | Move among type, representation, decimal places, and row filter |
| `PgUp`/`PgDn` | Scroll when content exceeds the viewport |
| `Enter` | Apply the highlighted option, or start/finish editing decimal / filter |
| Type / `Backspace` | Edit decimal format or filter when that row is focused |
| `q` | Close |

Shows type options (filtered by inferred data), numeric representation when relevant, decimal places, a per-column row filter, and progressive statistics.

### Row JSON (`r`)

Shows the selected row as a pretty-printed JSON object. While open, table navigation is disabled.

| Key | Action |
|-----|--------|
| `↑`/`↓` / `j`/`k` | Scroll vertically |
| `←`/`→` / `h`/`l` | Scroll horizontally |
| `PgUp`/`PgDn` | Page vertically |
| `Home`/`End` | Jump to start / end |
| `q` or `r` | Close |

Mouse: drag the **title bar** to move, drag the **bottom-right corner** to resize (min 30×8), use scrollbars or wheel for overflow. Opening row JSON closes column info (and vice versa). Position and size persist for the session after the first drag or resize.

## Mouse

| Target | Action |
|--------|--------|
| Table header border | Drag to resize column width (4–64) |
| Table header | Select column; **Ctrl+click** multi-select; **right-click** context menu |
| Table body | Click to select; **drag** for a cell rectangle; **Ctrl+click** toggles individual cells |
| Row gutter | **Right-click** context menu (select / hide / pin) |
| Table wheel | Move selected row ±3 |
| Scrollbars | Drag thumb or track; wheel over a scrollbar scrolls that pane |
| Column list | Click / Ctrl+click / right-click context menu; drag left border to resize; wheel scrolls the list |
| Column info / row JSON | As described above |

### Selection and visibility

- Multi-selected columns use a blue column highlight (`◆` in the sidebar); multi-selected rows use a blue row background.
- Cell ranges and Ctrl+selected cells highlight in blue; the active cell stays yellow.
- Row/column multi-select and cell selection are mutually exclusive — starting one clears the other.
- Hidden columns stay in the sidebar with a dim `·` prefix (at the end of the list) but are omitted from the table.
- Pinned columns show `▐` and stay on the left; pinned rows show `▐` and stay at the top.
- At least one column and one row must remain visible; `:hide` errors if it would hide everything.

Context menus (sidebar, header, or row gutter): **↑/↓**, **Enter**, **Esc**/`q`, or click an item. **Ctrl+right-click** adds to multi-select.

## Column list scrolling

The sidebar uses its own `column_list_offset`, independent of table selection, so wheel scrolling is not forced to keep the selected column in view.

## Run

```bash
pixi run csv
pixi run csv test-data/generated/test_1000x100.csv
./target/release/csv
./target/release/csv tui file.csv
```

## Related

- [Data loading](../reference/data-loading.md)
- [CSV parsing & column types](../reference/csv-parsing.md)
- [Web UI](web.md)
- [Architecture](../architecture.md) — `TableViewState` and shared model
