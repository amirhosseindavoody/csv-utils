# Web UI

Browser table explorer backed by the same `AppModel` as the TUI. Start it from the TUI with **`:web`** while a file is open.

## Usage

1. Open a CSV in the TUI: `pixi run csv test-data/generated/test_1000x100.csv`
2. Type **`:web`** and press Enter
3. The terminal view closes; the URL is printed (for example `http://127.0.0.1:54321/`)
4. Open that URL in a browser
5. Press **Ctrl+C** in the terminal to stop the server

The server binds to `127.0.0.1` on a free port. Layout dimensions are taken from the terminal viewport at handoff time.

## Page layout

Same logical regions as the TUI: title bar, data table, column sidebar, and hint footer.

## Theme

- **Default:** OS light/dark via `prefers-color-scheme`
- **Theme button:** cycles System → Light → Dark
- Stored in `localStorage` (`csv-utils-theme`) until returned to System

## Keyboard

Mirrors the TUI: `↑↓←→`, `PgUp`/`PgDn`, `Home`/`End`, `c` (column info), `r` (row JSON), `?`, `q` (close panel).

## Mouse

- Click table cells or column list items to select
- Wheel on the table or column list scrolls rows or the sidebar
- Column info and row JSON panels scroll when content overflows
- Row JSON: drag the title to move, resize from the corner
- Table and sidebar scroll indicators support thumb/track drag
- Drag a column header’s **right edge** to resize (4–64 chars); synced on mouse release

## JSON API

| Route | Method | Purpose |
|-------|--------|---------|
| `/` | GET | Single-page UI |
| `/api/state` | GET | Current `ClientView` JSON |
| `/api/action` | POST | Apply a `ViewAction` |

Example actions:

```json
{"action": "row_delta", "value": -1}
{"action": "select_cell", "value": {"row": 0, "col": 2}}
{"action": "set_column_width", "value": {"col": 0, "width": 24}}
{"action": "open_column_info"}
{"action": "close_column_info"}
{"action": "open_row_json"}
{"action": "close_row_json"}
{"action": "column_info_focus_delta", "value": 1}
{"action": "column_info_apply"}
{"action": "set_column_kind", "value": {"col": 0, "kind": "float"}}
{"action": "set_numeric_repr", "value": {"col": 0, "repr": "scientific"}}
{"action": "set_column_decimal_format", "value": {"col": 0, "format": ".3"}}
{"action": "set_row_offset", "value": 128}
{"action": "set_col_offset", "value": 4}
{"action": "set_column_list_offset", "value": 12}
```

The page polls `/api/state` while the background scan runs (`scan_done: false`). Pinned columns and rows match the TUI. Scroll metadata counts only unpinned columns and rows.

Implementation: `csv-utils/src/web/server.rs`, embedded UI in `csv-utils/src/web/index.html`.

## Related

- [Architecture](../architecture.md#shared-view-model)
- [TUI](tui.md)
