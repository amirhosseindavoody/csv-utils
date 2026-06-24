# Web UI

Browser table explorer backed by the same `AppModel` as the TUI. Binary: **`csv-utils-web`** (`target/release/csv-utils-web`).

## Usage

```
csv-utils-web [file.csv] [--host HOST] [--port PORT]
```

| Flag | Default | Meaning |
|------|---------|---------|
| `--host` | `127.0.0.1` | Bind address (use `0.0.0.0` for LAN) |
| `--port` | `8080` | TCP port |

```bash
pixi run web -- test-data/generated/test_1000x100.csv
pixi run web -- --host 0.0.0.0 --port 8080 file.csv
pixi run web-tui   # shortcut with test CSV
```

Open `http://127.0.0.1:8080/` (or your `--host`/`--port`). Ctrl+C stops the server and joins the background scan thread.

## Page layout

Same logical regions as the TUI: title/meta bar, data table, column sidebar, hint footer. Layout constants in the server: 24 visible rows, 110-char table width, 20 sidebar lines.

## Theme

- **Default:** follows OS light/dark via `prefers-color-scheme`
- **Header Theme button:** cycles **System → Light → Dark**
- Stored in `localStorage` (`csv-utils-theme`) until set back to System

## Keyboard

Mirrors the TUI: `↑↓←→`, `PgUp`/`PgDn`, `Home`/`End`, `c` (column info), `?`, `q` (close panel).

## Mouse

- Click table cells or column list items to select
- Wheel on table / column list scrolls rows / sidebar
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
```

The page polls `/api/state` while the background scan runs (`scan_done: false`).

Implementation: `csv-utils-web/src/server.rs`, embedded UI in `index.html`.

## Related

- [Architecture](../architecture.md#shared-view-model)
- [TUI](tui.md) — terminal counterpart
