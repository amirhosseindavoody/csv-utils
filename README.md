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

After install, `csv` is available in the pixi environment.

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
pixi run csv
pixi run run -- stats sample.csv
pixi run run -- unique sample.csv city,active 100
pixi run run -- filter sample.csv city=Tehran,active=true 25
pixi run run -- filter sample.csv age>30 50
pixi run run -- filter sample.csv "name contains Ali" 20
pixi run run -- filter sample.csv "city in Tehran|Paris" 20
pixi run run -- json sample.csv 10
pixi run csv test-data/generated/test_1000x100.csv
```

In the TUI, press `?` for help. Type `:web` to hand off to the browser UI (terminal view closes). Press `q` to return to the file picker from a file, then `q` again to quit.

Direct cargo usage (from repo root):

```bash
cargo build --release
./target/release/csv
./target/release/csv test-data/generated/test_1000x100.csv
```

## Testing

```bash
pixi run gen-test-data
pixi run test
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

Artifact: `dist/csv-utils-*.conda`. See [docs/development/build.md](docs/development/build.md#conda-package).

## Status

- CLI: `stats`, `unique`, `json`, `filter`, `tui`
- TUI: ratatui table explorer with progressive row loading
- Web UI: `:web` in the TUI hands off to the browser (terminal view closes)
- Core library (`csv-utils-core`) exposes `AppModel`, `ViewAction`, and `ClientView` for TUI and web frontends
