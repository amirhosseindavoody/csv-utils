# Design: settings config file

Status: **implemented**.

csv-utils reads a JSON settings file from the **current working directory** when
TUI or web starts. If the file is missing, it is created with documented defaults.

## File location and name

| Property | Value |
|---|---|
| Filename | `csv-utils.json` |
| Directory | Process working directory at startup |
| Created when | First successful `AppModel::open` (TUI / web) if file absent |

Implementation: `csv-utils-core/src/settings.rs`

## Default contents

On first run, the file is written as pretty-printed JSON:

```json
{
  "display": {
    "numeric_decimal_format": ".3"
  },
  "file_picker": {
    "file_extensions": ["csv", "dat"]
  }
}
```

| Field | Meaning |
|---|---|
| `display.numeric_decimal_format` | Default decimal format for numeric columns |
| `file_picker.file_extensions` | File extensions shown in the TUI file picker by default (without leading dot) |

## Decimal format syntax

The format string uses a **leading dot + digit count**:

| Value | Meaning |
|---|---|
| `.3` | Up to **3 digits after the decimal point** (default) |
| `.0` | No fractional digits (integers shown without `.0` when they fit) |
| `.12` | Up to 12 digits (maximum allowed) |

Invalid values are rejected when applied from the column info panel; the previous
value is kept.

Parsing: `settings::parse_decimal_format`.

## File picker extensions

The TUI file picker filters directory listings to `file_picker.file_extensions`
by default (`.csv` and `.dat`). Directories are always shown. Override the list
in `csv-utils.json`; values may include or omit a leading dot (`csv` and `.csv`
are equivalent).

In the picker, type **`:all`** or **`:a`** to show every file, or **`:filter`**
/ **`:f`** to restore the configured extension filter.

## How settings are used

```text
  csv-utils.json (cwd)
        │
        ▼
  AppModel.settings          ← loaded once at open
        │
        ├── display.numeric_decimal_format → column decimal default
        │
        ├── file_picker.file_extensions → TUI file picker filter
        │
        └── column info panel shows default when override is None
```

| Scope | Storage | Lifetime |
|---|---|---|
| **Global default** | `csv-utils.json` | Persistent on disk |
| **Per-column override** | `TableViewState.column_decimal_formats` | Session only (cleared on new file) |

Per-column overrides are set in the column info panel (`c` → **Decimal places**
text field). They do not write back to `csv-utils.json` automatically.

## Column info panel

When the column is numeric (int/float, or inferred as such):

1. **Type** — text / date / int / float / auto
2. **Representation** — general / scientific
3. **Decimal places** — text input, default from config (e.g. `.3`)

TUI: focus the row, press **Enter** to edit, type (e.g. `.5`), **Enter** to apply.

Web: edit the input; **change** (blur) posts `set_column_decimal_format`.

Display code uses the resolved format when formatting cells (`display.rs`).

## Error handling

| Situation | Behavior |
|---|---|
| File missing | Create with defaults |
| File unreadable / invalid JSON | Use in-memory defaults; do not overwrite |
| Cannot create file (read-only cwd) | Use in-memory defaults silently |

## Future extensions

The JSON schema is intentionally small. Likely additions:

- Persist per-column overrides under a `columns` key
- Theme / TUI defaults
- CLI default limits

New fields should use `#[serde(default)]` so older config files keep working.

## Related

- [CSV parsing & display](../reference/csv-parsing.md)
- [TUI column info](../features/tui.md)
- [Web UI](../features/web.md)
