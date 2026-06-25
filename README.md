# csv-utils

High-performance CSV utility with CLI and interactive TUI (Rust + ratatui).

Design and behavior: **[docs/index.md](docs/index.md)** (structured guide; [DESIGN.md](docs/DESIGN.md) redirects there).

## Prerequisites

- [Pixi](https://pixi.sh/latest/)

## Install with pixi

In another pixi workspace, enable git source builds and add csv-utils from GitHub:

```toml
# pixi.toml
[workspace]
preview = ["pixi-build"]
```

```bash
pixi add --git https://github.com/amirhosseindavoody/csv-utils.git --branch main csv-utils
```

After install, `csv` and `csv-utils-web` are available in the pixi environment.

Install globally (available in the user's PATH):

```bash
pixi global install --git https://github.com/amirhosseindavoody/csv-utils.git --branch main csv-utils
```

## Setup

```bash
pixi install
```

## Run

```bash
pixi run build
pixi run run -- stats sample.csv
pixi run run -- unique sample.csv city,active 100
pixi run run -- filter sample.csv city=Tehran,active=true 25
pixi run run -- filter sample.csv age>30 50
pixi run run -- filter sample.csv "name contains Ali" 20
pixi run run -- filter sample.csv "city in Tehran|Paris" 20
pixi run run -- json sample.csv 10
pixi run tui test-data/generated/test_1000x100.csv
pixi run web-tui   # browser UI at http://127.0.0.1:8080/
```

In the TUI, press `?` for help.

Direct cargo usage (from repo root):

```bash
cargo build --release
./target/release/csv tui test-data/generated/test_1000x100.csv
./target/release/csv-utils-web test-data/generated/test_1000x100.csv
```

## Testing

```bash
pixi run gen-test-data
pixi run test
pixi run tui test-data/generated/test_1000x100.csv
```

Capture a TUI snapshot (PTY via `script(1)`):

```bash
pixi run test-tui-large-capture
```

## Conda package

Build a `.conda` package (includes `csv` and `csv-utils-web` binaries):

```bash
pixi run conda-package
```

Artifact: `dist/csv-utils-*.conda`. See [docs/development/build.md](docs/development/build.md#conda-package).

## Status

- CLI: `stats`, `unique`, `json`, `filter`, `tui`
- TUI: ratatui table explorer with progressive row loading
- Web UI: `csv-utils-web` serves the same model in a browser (`pixi run web-tui`)
- Core library (`csv-utils-core`) exposes `AppModel`, `ViewAction`, and `ClientView` for TUI and web frontends
