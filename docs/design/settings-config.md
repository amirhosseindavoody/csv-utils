# Design: settings config file

csv-utils loads settings from a **global** file in the user config directory, then
applies optional **local** overrides from the process working directory. Local
fields win over global fields at each JSON key (deep merge for nested objects).

## File locations

| Layer | Path | Role |
|---|---|---|
| **Global** | `~/.config/csv-utils/csv-utils.json` | User defaults; created on first TUI/web open if missing |
| **Local** | `./csv-utils.json` (cwd at startup) | Optional project overrides; never auto-created |

Resolution order:

1. Load global (or create with defaults when the global file is missing).
2. If `./csv-utils.json` exists in the working directory, merge it on top.
3. Use the merged result in `AppModel.settings`.

Environment overrides (mainly for tests):

| Variable | Effect |
|---|---|
| `CSV_UTILS_CONFIG_DIR` | Directory for the global settings file instead of `~/.config/csv-utils` |
| `XDG_CONFIG_HOME` | When set, global dir is `$XDG_CONFIG_HOME/csv-utils` |
| `HOME` | Fallback: `$HOME/.config/csv-utils` |

Implementation: `csv-utils-core/src/settings.rs` (`load_merged`, `merge_settings`).

## Merge semantics

Merge is **field-level** on JSON objects:

- Top-level keys (`display`, `file_picker`) merge recursively.
- Leaf values in the local file replace the global value.
- Keys omitted in the local file keep the global value.

Example:

**Global** (`~/.config/csv-utils/csv-utils.json`):

```json
{
  "display": {
    "numeric_decimal_format": ".2",
    "show_column_borders": true
  },
  "file_picker": {
    "file_extensions": ["csv", "dat"]
  }
}
```

**Local** (`./csv-utils.json` in a project directory):

```json
{
  "display": {
    "numeric_decimal_format": ".5"
  },
  "file_picker": {
    "file_extensions": ["tsv", "csv"]
  }
}
```

**Merged result:**

| Field | Value | Source |
|---|---|---|
| `display.numeric_decimal_format` | `.5` | local |
| `display.show_column_borders` | `true` | global (unchanged) |
| `file_picker.file_extensions` | `["tsv", "csv"]` | local (whole array replaced) |

## Default contents

When the global file is created for the first time:

```json
{
  "display": {
    "numeric_decimal_format": ".3",
    "show_column_borders": true
  }
}
```

Optional extension filter (omit `file_picker` or use an empty list to show all files):

```json
{
  "file_picker": {
    "file_extensions": ["csv", "dat"]
  }
}
```

| Field | Meaning |
|---|---|
| `display.numeric_decimal_format` | Default decimal format for numeric columns |
| `display.show_column_borders` | When `true`, the TUI draws `│` column lines and a `─` header rule in the one-character gaps between columns (default). When `false`, gaps stay blank |
| `file_picker.file_extensions` | Optional. When non-empty, the file picker can filter to these extensions (`:filter`); omit or `[]` to list all files by default |

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

By default the TUI file picker lists **all** files and directories (dotfiles are
still hidden). To enable extension filtering, set a non-empty
`file_picker.file_extensions` array in settings. Values may include or omit a
leading dot (`csv` and `.csv` are equivalent).

When extensions are configured:

- The picker starts filtered to those extensions
- **`:all`** / **`:a`** shows every file
- **`:filter`** / **`:f`** restores the configured extension filter

When `file_extensions` is omitted or empty, the picker always shows all files and
`:filter` has no effect.

## How settings are used

```text
  ~/.config/csv-utils/csv-utils.json   (global; created if missing)
              │
              ▼ deep merge (local wins per field)
  ./csv-utils.json                     (optional local overrides)
              │
              ▼
  AppModel.settings                    ← loaded once at open
        │
        ├── display.numeric_decimal_format → column decimal default
        │
        ├── display.show_column_borders → TUI column gap lines on open
        │
        ├── file_picker.file_extensions → optional TUI file picker extension filter
        │
        └── column info panel shows default when override is None
```

| Scope | Storage | Lifetime |
|---|---|---|
| **Global defaults** | `~/.config/csv-utils/csv-utils.json` | Persistent; auto-created on first open |
| **Project overrides** | `./csv-utils.json` | Optional; manual edit only |
| **Per-column override** | `TableViewState.column_decimal_formats` | Session only (cleared on new file) |

Per-column overrides are set in the column info panel (`c` → **Decimal places**
text field). They do not write back to either settings file automatically.

## Column info panel

When the column is numeric (int/float, or inferred as such):

1. **Type** — text / date / int / float / auto
2. **Representation** — general / scientific
3. **Decimal places** — text input, default from merged config (e.g. `.3`)

TUI: focus the row, press **Enter** to edit, type (e.g. `.5`), **Enter** to apply.

Web: edit the input; **change** (blur) posts `set_column_decimal_format`.

Display code uses the resolved format when formatting cells (`display.rs`).

## Error handling

| Situation | Behavior |
|---|---|
| Global file missing | Create global file with defaults |
| Global unreadable / invalid JSON | Use in-memory defaults; do not overwrite |
| Cannot create global file (read-only home) | Use in-memory defaults silently |
| Local file missing | Use global settings only |
| Local unreadable / invalid JSON | Ignore local file; use global settings only |
| Local present but empty `{}` | Same as global (no overrides) |

## Migration from cwd-only config

Older releases wrote `./csv-utils.json` in the working directory only. That file
still works as a **local override**. On first run after this change, a global file
is created in the home config directory; existing project-level `csv-utils.json`
files continue to override it when you launch csv-utils from that directory.

To promote a project file to global defaults, copy its contents to
`~/.config/csv-utils/csv-utils.json` and trim any keys you do not want globally.

## Possible extensions

The JSON schema is intentionally small. Natural additions include per-column
overrides, theme defaults, CLI default limits, and an explicit command to write
session changes back to global or local config. New fields should use
`#[serde(default)]` so older config files keep working.

## Related

- [CSV parsing & display](../reference/csv-parsing.md)
- [TUI column info](../features/tui.md)
- [Web UI](../features/web.md)
