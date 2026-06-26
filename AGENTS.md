# AGENTS.md

## Cursor Cloud specific instructions

This is a Rust workspace (`csv-utils`) managed with **Pixi**. It produces one product with three frontends:

- `csv-utils-core` — shared library (CSV parsing, data loading, `AppModel`/`ViewAction`/`ClientView`).
- `csv-utils` — the `csv` binary: CLI subcommands (`stats`, `unique`, `json`, `filter`) plus the interactive ratatui TUI.
- `csv-utils-web` — the `csv-utils-web` binary: an Axum/Tokio HTTP server that serves the same model in the browser.

### Toolchain (important)

- The system `cargo`/`rustc` is **1.83 and too old** (the dependency tree needs edition 2024 / Rust ≥1.96). Running bare `cargo …` fails with an `edition2024` error.
- Always go through Pixi, which provides the conda `rust` toolchain. Use `pixi run <task>` for the predefined tasks, or `pixi run -- cargo <args>` for ad-hoc cargo commands.

### Build / test / lint / run

Predefined tasks live in `pixi.toml` (see also `docs/development/build.md`):

- Build: `pixi run build`
- Generate test data (writes `test-data/generated/*.csv`): `pixi run gen-test-data`
- CLI: `pixi run csv -- <subcommand> <file> …` (e.g. `pixi run csv -- stats test-data/generated/test_1000x100.csv`)
- TUI: `pixi run tui [file]`
- Lint: `pixi run -- cargo clippy --release` (clippy currently emits warnings only, no errors)

### Gotchas

- **Tests must run single-threaded.** `pixi run test` (plain `cargo test`) is flaky because the `csv-utils-core` settings tests mutate the process-global current directory in parallel (e.g. `settings::tests::invalid_local_file_is_ignored` intermittently fails with `.5` vs `.4`). Run `pixi run -- cargo test -- --test-threads=1` for reliable results — all 75 tests pass that way.
- **The `web` / `web-tui` pixi tasks are broken** (a stray single quote in the `web` task definition makes Pixi fail with "Expected closing single quote"). To run the web server, invoke the binary directly after `pixi run build`:
  - `./target/release/csv-utils-web test-data/generated/test_1000x100.csv` (serves `http://127.0.0.1:8080/`; `--host`/`--port` flags available), or
  - `pixi run -- cargo run --release -p csv-utils-web -- test-data/generated/test_1000x100.csv`
- The README references a `pixi run run` task; the actual task is named `csv` (there is no `run` task).
- Web JSON API: `GET /api/state`, `POST /api/action` (body like `{"action":"select_cell","value":{"row":0,"col":2}}`); see `docs/features/web.md`.
