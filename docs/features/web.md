# Web UI

Browser table explorer backed by the same `AppModel` as the TUI. Start it from the TUI with **`:web`** while a file is open.

## Usage

1. Open a CSV in the TUI: `pixi run csv test-data/generated/test_1000x100.csv`
2. Type **`:web`** and press Enter
3. The terminal view closes; the URL is printed (e.g. `http://127.0.0.1:54321/`)
4. Open that URL in your browser
5. Press **Ctrl+C** in the terminal to stop the server

The server binds to `127.0.0.1` on a free port chosen by the OS. Layout dimensions are taken from the terminal viewport at handoff time.

## Page layout

Same logical regions as the TUI: title/meta bar, data table, column sidebar, hint footer.

## Theme

- **Default:** follows OS light/dark via `prefers-color-scheme`
- **Header Theme button:** cycles **System → Light → Dark**
- Stored in `localStorage` (`csv-utils-theme`) until set back to System

## Keyboard

Mirrors the TUI: `↑↓←→`, `PgUp`/`PgDn`, `Home`/`End`, `c` (column info), `?`, `q` (close panel).

## Mouse

- Click table cells or column list items to select
- Wheel on table / column list scrolls rows / sidebar
- Column info panel scrolls when content exceeds the viewport (wheel or scrollbar)
- Table and column sidebar show scroll indicators when row/column lists extend past the viewport; drag the thumb or track to scroll (wheel still drives navigation via the API)
- Drag column header **right edge** to resize (4–64 chars); synced on mouse release via API

## JSON API

| Route | Method | Purpose |
|-------|--------|---------|
| `/` | GET | Single-page browser UI |
| `/api/state` | GET | Current `ClientView` JSON |
| `/api/action` | POST | Apply a `ViewAction` |

Example action:

```json
{"action": "row_delta", "value": -1}
{"action": "select_cell", "value": {"row": 0, "col": 2}}
{"action": "set_column_width", "value": {"col": 0, "width": 24}}
{"action": "open_column_info"}
{"action": "close_column_info"}
{"action": "column_info_focus_delta", "value": 1}
{"action": "column_info_apply"}
{"action": "set_column_kind", "value": {"col": 0, "kind": "float"}}
{"action": "set_numeric_repr", "value": {"col": 0, "repr": "scientific"}}
{"action": "set_column_decimal_format", "value": {"col": 0, "format": ".3"}}
{"action": "set_row_offset", "value": 128}
{"action": "set_col_offset", "value": 4}
{"action": "set_column_list_offset", "value": 12}
```

The page polls `/api/state` while the background scan runs (`scan_done: false`). Pinned columns appear in the table’s fixed left segment (same as the TUI); sidebar items include a `pinned` boolean. Horizontal scroll metadata (`table_cols_scroll`) counts only unpinned columns.

Implementation: `csv-utils/src/web/server.rs`, embedded UI in `csv-utils/src/web/index.html`.

## Related

- [Architecture](../architecture.md#shared-view-model)
- [TUI](tui.md) — terminal counterpart (`:web` command)
