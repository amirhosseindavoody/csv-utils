# csv-utils

High-performance CSV utility with a streaming **CLI**, interactive **TUI**, and optional **web UI** — written in Rust.

**Documentation:** [amirhosseindavoody.github.io/csv-utils](https://amirhosseindavoody.github.io/csv-utils/) · [docs/index.md](docs/index.md)

## Features

| Surface | What it does |
|---------|--------------|
| **CLI** | `stats`, `unique`, `json`, `filter` — stream large files without loading into memory |
| **TUI** | Full-screen table explorer with progressive loading, column sidebar, filters, and mouse support |
| **Web UI** | Same explorer in the browser via `:web` (local HTTP server) |

## Quick start

### Prerequisites

- [Pixi](https://pixi.sh/latest/)

### From source

```bash
git clone https://github.com/amirhosseindavoody/csv-utils.git
cd csv-utils
pixi install
pixi run build
pixi run csv                          # TUI file picker
pixi run csv test-data/generated/test_1000x100.csv
```

### Install with pixi (another workspace)

Enable git source builds, then add from GitHub:

```toml
# pixi.toml
[workspace]
preview = ["pixi-build"]
```

```bash
pixi add --git https://github.com/amirhosseindavoody/csv-utils.git --branch main csv-utils
```

After install, `csv` is available in the pixi environment.

Install globally (adds `csv` to your PATH):

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

Filter operators: `=`, `!=`, `>`, `<`, `contains`, `in`. Comma-separated conditions are ANDed. See [CLI reference](docs/features/cli.md).

### TUI

```bash
pixi run csv                          # file picker
pixi run csv sample.csv               # open file directly
```

- Press **`?`** for help
- Type **`:web`** to open the browser UI (terminal view closes)
- Press **`q`** to return to the file picker from a file, then **`q`** again to quit

See [TUI guide](docs/features/tui.md) for keyboard and mouse bindings.

### Direct cargo (from repo root)

Pixi provides Rust ≥ 1.96; bare system `cargo` may be too old:

```bash
pixi run build
./target/release/csv
./target/release/csv test-data/generated/test_1000x100.csv
```

## Testing

```bash
pixi run gen-test-data
pixi run -- cargo test -- --test-threads=1
pixi run csv test-data/generated/test_1000x100.csv
```

Capture a TUI snapshot (PTY via `script(1)`):

```bash
pixi run test-tui-large-capture
```

## Conda package

Build a `.conda` package (includes the `csv` binary):

```bash
pixi run conda-package
```

Artifact: `dist/csv-utils-*.conda`. See [Build & packaging](docs/development/build.md).

## Project structure

```
csv-utils-core/   # shared library (parsing, preview, AppModel)
csv-utils/        # `csv` binary (CLI + TUI + web server)
docs/             # documentation (also published to GitHub Pages)
```

## License

MIT — see [LICENSE](LICENSE).
