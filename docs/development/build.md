# Build & packaging

Development uses [Pixi](https://pixi.sh/latest/) for the Rust toolchain and task runner. The system `cargo` on many machines is too old for this workspace (edition 2024 / Rust ≥ 1.96); always prefer `pixi run …`.

## Prerequisites

- [Pixi](https://pixi.sh/latest/)
- Optional: [rustup](https://rustup.rs/) if you run bare `cargo` outside pixi

## Setup

```bash
pixi install          # conda env with rust ≥ 1.96, python for scripts
pixi run build        # release build → target/release/csv
```

## Pixi tasks

Defined in `pixi.toml` at the repo root:

| Task | Command | Purpose |
|------|---------|---------|
| `build` | `cargo build --release` | Build the `csv` binary |
| `csv` | `cargo run --release -p csv-utils --` | Run the binary; append args after `--` |
| `test` | `cargo test` | Run the test suite |
| `gen-test-data` | `python scripts/generate_test_data.py` | Write `test-data/generated/*.csv` |
| `test-tui-small` | `pixi run csv test-data/generated/test_1000x100.csv` | Open small test file in TUI |
| `test-tui-large` | `pixi run csv test-data/generated/test_10000x1000.csv` | Open large test file in TUI |
| `test-tui-large-capture` | Build + `scripts/capture_tui_snapshot.py` | Capture TUI snapshot via PTY |
| `conda-package` | `pixi publish --target-dir dist` | Build `.conda` package |
| `update-version` | `scripts/update-version.sh` | Bump version across manifests |

### Examples

```bash
pixi run build
pixi run csv -- stats sample.csv
pixi run csv test-data/generated/test_1000x100.csv
pixi run gen-test-data
pixi run gen-test-data --datasets 1000x100
```

Ad-hoc cargo (from repo root):

```bash
pixi run -- cargo clippy --release
pixi run -- cargo test -- --test-threads=1
```

**Note:** Run tests single-threaded for reliability — some settings tests mutate the process-global current directory and can flake in parallel:

```bash
pixi run -- cargo test -- --test-threads=1
```

## Workspace layout

```
Cargo.toml              # workspace root
csv-utils-core/         # shared library (parsing, preview, AppModel)
csv-utils/              # `csv` binary (CLI + TUI + embedded web server)
recipe/recipe.yaml      # conda package recipe (rattler-build)
scripts/                # test data generator, TUI capture
test-data/generated/    # synthetic CSVs (gitignored output)
docs/                   # user and developer documentation
```

## Conda package

Build a `.conda` artifact that installs the `csv` binary:

```bash
pixi run conda-package
```

Output: `dist/csv-utils-*.conda`. The recipe lives in `recipe/recipe.yaml` and uses `pixi-build-rattler-build` with the workspace `Cargo.lock`.

Install from another pixi workspace:

```toml
# pixi.toml
[workspace]
preview = ["pixi-build"]
```

```bash
pixi add --git https://github.com/amirhosseindavoody/csv-utils.git --branch main csv-utils
```

Global install (adds `csv` to PATH):

```bash
pixi global install --git https://github.com/amirhosseindavoody/csv-utils.git --branch main csv-utils
```

## Documentation site

User docs are published to GitHub Pages via mdBook. Source lives in `docs/`; `book.toml` and `docs/SUMMARY.md` define the book structure.

Local preview (requires [mdBook](https://rust-lang.github.io/mdBook/)):

```bash
mdbook serve --open
```

CI builds on every push to `main` and deploys to the `gh-pages` branch. See `.github/workflows/pages.yml`.

## Related

- [Getting started](../getting-started.md)
- [Architecture](../architecture.md)
- [Test data generation](../test-data-generation.md)
