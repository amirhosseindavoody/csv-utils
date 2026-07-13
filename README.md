# csv-utils

High-performance CSV utility with a streaming **CLI**, interactive **TUI**, and optional **web UI** — written in Rust.

**Docs:** [amirhosseindavoody.github.io/csv-utils](https://amirhosseindavoody.github.io/csv-utils/) · [docs/index.md](docs/index.md)

## What it does

| Surface | Role |
|---------|------|
| **CLI** | `stats`, `unique`, `json`, `filter` — stream large files without loading them into memory |
| **TUI** | Full-screen table explorer: progressive load, filters, sort, pin/hide, row JSON, mouse support |
| **Web UI** | Same explorer in the browser via `:web` (local HTTP server) |

## Quick start

Requires [Pixi](https://pixi.sh/latest/).

```bash
git clone https://github.com/amirhosseindavoody/csv-utils.git
cd csv-utils
pixi install
pixi run build
pixi run csv                          # TUI file picker
pixi run csv test-data/generated/test_1000x100.csv
```

### Install into another pixi workspace

```toml
# pixi.toml
[workspace]
preview = ["pixi-build"]
```

```bash
pixi add --git https://github.com/amirhosseindavoody/csv-utils.git --branch main csv-utils
```

Or install globally (adds `csv` to your PATH):

```bash
pixi global install --git https://github.com/amirhosseindavoody/csv-utils.git --branch main csv-utils
```

## Usage

### CLI

```bash
pixi run csv -- stats sample.csv
pixi run csv -- unique sample.csv city,active 100
pixi run csv -- filter sample.csv city=Tehran,active=true 25
pixi run csv -- filter sample.csv age>30 50
pixi run csv -- filter sample.csv "name contains Ali" 20
pixi run csv -- filter sample.csv "city in Tehran|Paris" 20
pixi run csv -- json sample.csv 10
```

Filter operators: `=`, `!=`, `>`, `<`, `contains`, `in`. Comma-separated conditions are ANDed. See the [CLI reference](docs/features/cli.md).

### TUI

```bash
pixi run csv                          # file picker
pixi run csv sample.csv               # open a file directly
```

Useful keys: **`?`** help · **`c`** column info · **`r`** row as JSON · **`/`** find columns · **`:`** commands (`:filter`, `:sort`, `:web`, …) · **`q`** close panel / return to picker / quit.

See the [TUI guide](docs/features/tui.md) for the full keyboard and mouse reference.

Pixi supplies Rust ≥ 1.96. Prefer `pixi run …` over a system `cargo` that may be too old:

```bash
pixi run build
./target/release/csv
```

## Development

```bash
pixi run gen-test-data
pixi run -- cargo test -- --test-threads=1
pixi run conda-package                # → dist/csv-utils-*.conda
```

See [Getting started](docs/getting-started.md) and [Build & packaging](docs/development/build.md).

## Project layout

```
csv-utils-core/   # shared library (parsing, preview, AppModel)
csv-utils/        # `csv` binary (CLI + TUI + web server)
docs/             # documentation (published to GitHub Pages)
```

## License

MIT — see [LICENSE](LICENSE).
