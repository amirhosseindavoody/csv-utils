# Documentation

**[index.md](index.md)** — documentation hub (user guide, reference, developer docs).

**Published site:** [amirhosseindavoody.github.io/csv-utils](https://amirhosseindavoody.github.io/csv-utils/) (built from `main` via mdBook + GitHub Actions).

| Section | Documents |
|---------|-----------|
| **User guide** | [Getting started](getting-started.md), [UX overview](user-experience/overview.md), [CLI](features/cli.md), [TUI](features/tui.md), [Web](features/web.md) |
| **Reference** | [Data loading](reference/data-loading.md), [CSV parsing](reference/csv-parsing.md), [Limitations](reference/limitations.md) |
| **Design** | [Large-file preview](design/large-file-preview.md), [Row filtering](design/row-filtering.md), [Settings config](design/settings-config.md) |
| **Developer** | [Principles](principles.md), [Architecture](architecture.md), [Build](development/build.md) |
| **Other** | [Test data generation](test-data-generation.md) |

Local preview (requires [mdBook](https://rust-lang.github.io/mdBook/)):

```bash
mdbook serve --open
```

Navigation for the book is defined in [SUMMARY.md](SUMMARY.md). [DESIGN.md](DESIGN.md) redirects to the hub for backward compatibility.
