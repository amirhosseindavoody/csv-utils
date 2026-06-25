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

## Dependencies

Workspace (`Cargo.toml`): ratatui, crossterm, clap, anyhow, thiserror, axum, tokio, serde.

Pixi dev feature: Python (test data scripts), optional conda `rust` for packaging workflows.

## Test data

See [test-data-generation.md](../test-data-generation.md).

## Snapshots

`pixi run test-tui-large-capture` uses `scripts/capture_tui_snapshot.py` and PTY `script(1)` to write `artifacts/tui_snapshot_large.txt`.
