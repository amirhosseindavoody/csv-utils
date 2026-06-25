# Build & packaging

## Pixi tasks

| Task | Command |
|------|---------|
| Build (cargo) | `pixi run build` |
| Conda package | `pixi run conda-package` → `dist/csv-utils-*.conda` |
| Run CLI | `pixi run run -- stats file.csv` |
| TUI | `pixi run tui file.csv` |
| Web UI | `pixi run web -- file.csv` or `pixi run web-tui` |
| Unit tests | `pixi run test` |
| Generate test CSVs | `pixi run gen-test-data` |
| TUI snapshot | `pixi run test-tui-large-capture` → `artifacts/tui_snapshot_large.txt` |
| Bump version | `pixi run update-version` → semver in `Cargo.toml` and `pixi.toml` |

Version scheme (calendar + daily build counter):

| Meaning | Example |
|---------|---------|
| Human / calver label | `2026.06.24.0` (YYYY.MM.DD.N) |
| Stored in TOML (Cargo semver) | `2026.6.24+0` (YYYY.M.D+N) |

`N` starts at `0` each day and increments on every `update-version` run that day. Cargo and Pixi require [semver](https://semver.org/); the fourth dotted segment is encoded as build metadata (`+N`). Script: `scripts/update-version.sh`.

Tasks use `-p csv-utils` or `-p csv-utils-web` where needed (multi-binary workspace).

## Conda package

The repo is a Pixi **package** (`[package]` in `pixi.toml`) built with **`pixi-build-rattler-build`** and `recipe/recipe.yaml`.

```bash
pixi run conda-package
# equivalent:
pixi publish --target-dir dist
```

Output: `dist/csv-utils-<version>-<build>_0.conda` with `csv` and `csv-utils-web` in `$PREFIX/bin`. Build uses conda-forge `rust` and `gcc` (no rustup required for packaging).

Publish to an indexed local channel:

```bash
pixi publish --target-channel file:///path/to/my-channel
```

### conda-forge

Initial submission: [conda-forge/staged-recipes#33899](https://github.com/conda-forge/staged-recipes/pull/33899). After merge, CI creates `conda-forge/csv-utils-feedstock` and publishes to the `conda-forge` channel.

Install (once the feedstock is live):

```bash
conda install -c conda-forge csv-utils
```

Future releases:

1. Tag the commit on GitHub (e.g. `v2026.6.24+3` after `pixi run update-version`).
2. Open a PR on `conda-forge/csv-utils-feedstock` bumping `context.version`, the source URL, and `sha256` in `recipe/recipe.yaml`.

The feedstock recipe lives in staged-recipes / the feedstock repo (GitHub tarball source). The in-repo `recipe/recipe.yaml` uses `source.path` for local Pixi builds only.

## Dependencies

Workspace (`Cargo.toml`): ratatui, crossterm, clap, anyhow, thiserror, axum, tokio, serde.

Pixi dev feature: Python (test data scripts), optional conda `rust` for packaging workflows.

## Test data

See [test-data-generation.md](../test-data-generation.md).

## Snapshots

`pixi run test-tui-large-capture` uses `scripts/capture_tui_snapshot.py` and PTY `script(1)` to write `artifacts/tui_snapshot_large.txt`.
