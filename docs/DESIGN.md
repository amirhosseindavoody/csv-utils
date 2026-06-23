# Design documentation

> **This file is a compatibility alias.** The design spec now lives in a structured doc tree inspired by [Fresh](https://github.com/sinelaw/fresh/tree/master/docs).

**Start here: [docs/index.md](index.md)**

## Quick map (old → new)

| Former `DESIGN.md` section | New location |
|------------------------------|--------------|
| Purpose, design goals | [principles.md](principles.md) |
| Architecture, module map | [architecture.md](architecture.md) |
| Entry point, pixi quick ref | [getting-started.md](getting-started.md) |
| CLI commands | [features/cli.md](features/cli.md) |
| TUI | [features/tui.md](features/tui.md) |
| Web UI | [features/web.md](features/web.md) |
| Data loading | [reference/data-loading.md](reference/data-loading.md) |
| CSV parsing, column types | [reference/csv-parsing.md](reference/csv-parsing.md) |
| Known limitations | [reference/limitations.md](reference/limitations.md) |
| Build, conda package | [development/build.md](development/build.md) |
| Test data | [test-data-generation.md](test-data-generation.md) |

When you change behavior, update the relevant doc under `docs/` and [README.md](../README.md). See [.cursor/rules/keep-docs-in-sync.mdc](../.cursor/rules/keep-docs-in-sync.mdc).
