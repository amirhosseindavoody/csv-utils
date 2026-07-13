# Documentation

Published hub for csv-utils. Start at **[index.md](index.md)**.

**Live site:** [amirhosseindavoody.github.io/csv-utils](https://amirhosseindavoody.github.io/csv-utils/) (mdBook from `main` via GitHub Actions).

| Section | Documents |
|---------|-----------|
| **User guide** | [Getting started](getting-started.md), [UX overview](user-experience/overview.md), [CLI](features/cli.md), [TUI](features/tui.md), [Web](features/web.md) |
| **Reference** | [Data loading](reference/data-loading.md), [CSV parsing](reference/csv-parsing.md), [Limitations](reference/limitations.md) |
| **Design** | [Large-file preview](design/large-file-preview.md), [Row filtering](design/row-filtering.md), [Settings](design/settings-config.md), [Performance](design/performance-tui-responsiveness.md) |
| **Developer** | [Principles](principles.md), [Architecture](architecture.md), [Build](development/build.md), [Test data](test-data-generation.md) |

Local preview (requires [mdBook](https://rust-lang.github.io/mdBook/)):

```bash
mdbook serve --open
```

Book navigation is defined in [SUMMARY.md](SUMMARY.md). [DESIGN.md](DESIGN.md) is a short redirect for older links.
