# AGENTS.md

## Cursor Cloud specific instructions

This is a Rust workspace (`csv-utils`) managed with **Pixi**. It produces one product with two crates:

- `csv-utils-core` — shared library (CSV parsing, data loading, `AppModel`/`ViewAction`/`ClientView`).
- `csv-utils` — the `csv` binary: CLI subcommands (`stats`, `unique`, `json`, `filter`) plus the interactive ratatui TUI, with an embedded web server started via `:web`.

### Toolchain (important)

- The system `cargo`/`rustc` is **1.83 and too old** (the dependency tree needs edition 2024 / Rust ≥1.96). Running bare `cargo …` fails with an `edition2024` error.
- Always go through Pixi, which provides the conda `rust` toolchain. Use `pixi run <task>` for the predefined tasks, or `pixi run -- cargo <args>` for ad-hoc cargo commands.

### Build / test / lint / run

Predefined tasks live in `pixi.toml` (see also `docs/development/build.md`):

- Build: `pixi run build`
- Generate test data (writes `test-data/generated/*.csv`): `pixi run gen-test-data`
- CLI: `pixi run csv -- <subcommand> <file> …` (e.g. `pixi run csv -- stats test-data/generated/test_1000x100.csv`)
- TUI: `pixi run csv [file]` (or `pixi run csv -- tui [file]`)
- Web UI: type `:web` inside the TUI while a file is open (hands off to browser, terminal view closes)
- Lint: `pixi run -- cargo clippy --release` (clippy currently emits warnings only, no errors)

### Gotchas

- Tests must run single-threaded: `pixi run -- cargo test -- --test-threads=1` for reliable results.
- Web JSON API: `GET /api/state`, `POST /api/action` (body like `{"action":"select_cell","value":{"row":0,"col":2}}`); see `docs/features/web.md`.
