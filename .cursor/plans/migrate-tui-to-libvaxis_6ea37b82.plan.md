---
name: migrate-tui-to-libvaxis
overview: Replace the ncurses-based TUI with a libvaxis implementation while preserving current TUI behavior (row panel, column panel, keyboard+mouse interactions) and keeping CLI/bench paths unchanged.
todos:
  - id: add-libvaxis-dep
    content: Add libvaxis as a Zig dependency in build.zig and expose it to the TUI module.
    status: completed
  - id: drop-ncurses-linkage
    content: Remove ncurses/tinfo include/link/object setup from build.zig after libvaxis wiring works.
    status: completed
  - id: rewrite-tui-app
    content: Port src/tui/app.zig to libvaxis event loop and renderer while preserving current state behavior.
    status: completed
  - id: restore-input-parity
    content: Implement keyboard and mouse parity for grid, row panel, and right column panel (including independent column list scrolling).
    status: completed
  - id: validate-and-docs
    content: Run build/test parity checks and update README/runtime notes for libvaxis-based TUI.
    status: completed
isProject: false
---

# Migrate TUI to Libvaxis

## Scope
- Replace only the TUI runtime and rendering/input stack; keep CSV loading/parsing logic from [src/core/preview.zig](/home/amirhossein/csv-utils/src/core/preview.zig) as-is.
- Preserve parity with current behaviors in [src/tui/app.zig](/home/amirhossein/csv-utils/src/tui/app.zig):
  - streaming row load during startup,
  - table/grid navigation,
  - row detail panel,
  - right column list panel,
  - mouse click/wheel interactions.

## Implementation Plan
- Add `libvaxis` dependency in [build.zig](/home/amirhossein/csv-utils/build.zig) via Zig package/dependency workflow; wire it into the `csv-utils` executable module imports.
- Remove ncurses-specific link setup from [build.zig](/home/amirhossein/csv-utils/build.zig) (include/lib/object file wiring for `ncursesw/tinfow`) once libvaxis compiles for this target.
- Replace [src/tui/app.zig](/home/amirhossein/csv-utils/src/tui/app.zig) with a libvaxis app loop:
  - initialize terminal + renderer,
  - poll vaxis events (key, mouse, resize, tick),
  - maintain current UI state machine (selection/offsets/panel state),
  - render frame using vaxis primitives.
- Keep input semantics aligned with current TUI:
  - keys (`q`, arrows, PgUp/PgDn, Home/End, `r`),
  - mouse select/double-click/wheel for grid/panels,
  - independent column-panel scroll behavior.
- Ensure `tui` mode in [src/main.zig](/home/amirhossein/csv-utils/src/main.zig) remains the same public entrypoint (`csv-utils tui <file>`).
- Update docs/tasks references where needed (at least [README.md](/home/amirhossein/csv-utils/README.md), optionally [pixi.toml](/home/amirhossein/csv-utils/pixi.toml) if runtime notes/tasks need adjustment).

## Verification
- Build: `zig build` and `zig build run -- tui <csv>`.
- Functional parity smoke checks:
  - initial rows visible immediately and continue streaming,
  - selection and scrolling across large file,
  - row panel open/close + scrolling,
  - column panel visibility/selection + independent panel scroll,
  - mouse interactions for grid + panels.
- Regression check: `zig build test` for non-TUI core/CLI paths.

## Notes
- Keep `preview` threading model untouched to minimize risk; only swap front-end rendering/input layer.
- If libvaxis APIs differ by version, pin an explicit compatible revision in `build.zig` and document it in repo notes.