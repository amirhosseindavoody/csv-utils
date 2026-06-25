# csv-utils documentation

Living documentation for behavior, architecture, and development workflows. Update these docs in the same change as user-visible code changes.

Last verified against: `main` (layered settings config, June 2026).

---

## User guide

- **[Getting started](getting-started.md)** — install, binaries, first commands
- **[User experience overview](user-experience/overview.md)** — how the three surfaces (CLI, TUI, web) relate
- **[CLI](features/cli.md)** — `stats`, `unique`, `json`, `filter`
- **[TUI](features/tui.md)** — table explorer, keys, mouse, column resize
- **[Web UI](features/web.md)** — browser server, theme, JSON API

## Reference

- **[Data loading](reference/data-loading.md)** — preview buffer, background scan, APIs
- **[CSV parsing & display](reference/csv-parsing.md)** — `csv` crate, column types, cell formatting
- **[Known limitations](reference/limitations.md)** — current constraints and trade-offs

## Developer documentation

- **[Guiding principles](principles.md)** — goals and design values
- **[Architecture](architecture.md)** — crates, shared model, module map
- **[Large-file preview (design)](design/large-file-preview.md)** — mmap, offset index, `csv` crate loader
- **[Row filtering (design)](design/row-filtering.md)** — filter evaluation, caching, performance model
- **[Settings config (design)](design/settings-config.md)** — layered global + local `csv-utils.json`, decimal format defaults
- **[Build & packaging](development/build.md)** — pixi tasks, conda package, dependencies
- **[Test data generation](test-data-generation.md)** — synthetic CSV generator

## Quick links

| Topic | Document |
|-------|----------|
| Filter expression syntax | [features/cli.md](features/cli.md#filter-expressions) |
| TUI row / column filter | [design/row-filtering.md](design/row-filtering.md) |
| TUI keyboard bindings | [features/tui.md](features/tui.md#keyboard) |
| Web `/api/action` | [features/web.md](features/web.md#json-api) |
| Pixi / conda build | [development/build.md](development/build.md) |

When behavior changes, update the relevant section here, then mirror essentials in [README.md](../README.md).
