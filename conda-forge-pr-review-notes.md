# Review feedback guide — [staged-recipes#33899](https://github.com/conda-forge/staged-recipes/pull/33899)

Reviewer: **@pavelzw** (conda-forge Rust team)  
Reference template: [conda-forge Rust example recipe](https://conda-forge.org/docs/maintainer/example_recipes/rust/)

There are **5 actionable comments**. Comment 5 is general guidance that ties the others together.

---

## Comment 1 — Add `package_contents` tests (with `strict: true`)

**Location:** [`recipes/csv-utils/recipe.yaml`](https://github.com/conda-forge/staged-recipes/pull/33899#discussion_r3479718031) — `tests:` section  
**Review link:** [discussion_r3479718031](https://github.com/conda-forge/staged-recipes/pull/33899#discussion_r3479718031)

### What you have now

```yaml
tests:
  - script:
      - csv --help
      - csv-utils-web --help
```

### What the reviewer is asking

Add a **`package_contents`** test that checks the built `.conda` package actually contains the expected files — not just that commands run in a test environment.

With **`strict: true`**, the test also **fails if unexpected files** are installed (e.g. stray binaries, leftover build artifacts).

### Why this matters

| Test type | What it verifies |
|-----------|------------------|
| `script:` (`csv --help`) | Binaries work when installed in a test env |
| `package_contents:` | The **package artifact** contains exactly what you expect |

`--help` can pass even if you accidentally ship extra files. `package_contents` + `strict: true` catches packaging mistakes early — important for conda-forge, where many maintainers copy patterns from existing recipes.

### What we plan to change

```yaml
tests:
  - script:
      - csv --help
      - csv-utils-web --help
  - package_contents:
      bin:
        - csv
        - csv-utils-web
      strict: true
```

**Note:** The official Rust template also tests shell-completion files under `share/`. `csv-utils` does not expose a `completion --shell` subcommand today, so we skip those lines (the reviewer's template says to remove completion steps if unsupported).

---

## Comment 2 — Use `cargo-auditable`

**Location:** [`recipes/csv-utils/recipe.yaml`](https://github.com/conda-forge/staged-recipes/pull/33899#discussion_r3479718624) — `requirements.build`  
**Review link:** [discussion_r3479718624](https://github.com/conda-forge/staged-recipes/pull/33899#discussion_r3479718624)

### What you have now

```yaml
requirements:
  build:
    - ${{ compiler('rust') }}
    - ${{ compiler('c') }}
    - ${{ stdlib('c') }}
    - cargo-bundle-licenses
```

And in `build.sh`:

```bash
cargo install --no-track --locked --root "$PREFIX" --path csv-utils
cargo install --no-track --locked --root "$PREFIX" --path csv-utils-web
```

### What the reviewer is asking

1. Add **`cargo-auditable`** as a build dependency.
2. Build with **`cargo auditable install`** instead of plain **`cargo install`**.

### Why this matters

Rust binaries on conda-forge are often **statically linked** with many crates vendored in. **`cargo-auditable`** embeds a **Software Bill of Materials (SBOM)** in each binary so security tools can answer: *"What exact crate versions are inside this binary?"*

This is now **standard practice** on conda-forge for Rust packages — see the [official Rust template](https://conda-forge.org/docs/maintainer/example_recipes/rust/).

### What we plan to change

**`recipe.yaml` — add dependency:**

```yaml
requirements:
  build:
    - ${{ stdlib('c') }}
    - ${{ compiler('c') }}
    - ${{ compiler('rust') }}
    - cargo-bundle-licenses
    - cargo-auditable          # ← new
```

**Build script — use `cargo auditable install`:**

```bash
# Unix
cargo auditable install --locked --no-track --bins --root "$PREFIX" --path csv-utils
cargo auditable install --locked --no-track --bins --root "$PREFIX" --path csv-utils-web

# Windows (from template)
cargo auditable install --locked --no-track --bins --root %LIBRARY_PREFIX% --path csv-utils
cargo auditable install --locked --no-track --bins --root %LIBRARY_PREFIX% --path csv-utils-web
```

`--bins` installs all binaries declared in each crate's `Cargo.toml` (`csv` from the `csv-utils` crate, `csv-utils-web` from the web crate).

---

## Comment 3 — Template the source URL + rethink versioning

**Location:** [`recipes/csv-utils/recipe.yaml`](https://github.com/conda-forge/staged-recipes/pull/33899#discussion_r3479723080) — `source.url`  
**Review link:** [discussion_r3479723080](https://github.com/conda-forge/staged-recipes/pull/33899#discussion_r3479723080)

### What you have now

```yaml
context:
  version: "2026.6.24+2"

source:
  url: https://github.com/amirhosseindavoody/csv-utils/archive/v2026.6.24%2B2.tar.gz
  sha256: f911511a52d195a5330d78502ce663d340ad8a9fe8e3faa83cb40cfc89081769
```

The version appears in **three places**, but only one is templated:

- `context.version` ✅
- `package.version: ${{ version }}` ✅
- `source.url` ❌ hardcoded

### What the reviewer is asking

**Part A — Template the URL** so bumping the version is a single change (+ new `sha256`), matching the template:

```yaml
url: https://github.com/.../archive/refs/tags/v${{ version }}.tar.gz
```

**Part B — Rethink versioning** because `2026.6.24+2` may break the **conda-forge autotick bot** (`regro-correct-autotick`), which opens PRs when it detects new GitHub releases. It expects conventional semver tags like `v1.2.3`, not calver with `+` build metadata.

### Why this matters

**DRY for maintainers:** Hardcoded URLs drift from `context.version` and cause subtle bugs (wrong tarball, wrong checksum).

**Automation:** Without bot-friendly tags, **every release is a manual feedstock PR** — fine if you accept that, but worth deciding consciously.

### The `+` complication

Your Cargo/Pixi version is `2026.6.24+2`. GitHub archive URLs need `%2B` instead of `+`:

```
https://github.com/.../archive/v2026.6.24%2B2.tar.gz
```

A naive `v${{ version }}` produces `v2026.6.24+2`, which **404s** on GitHub.

### What we plan to change

**Short term (for this PR):** add a dedicated tag variable:

```yaml
context:
  version: "2026.6.24+2"
  tag: "v2026.6.24%2B2"   # URL-safe Git tag (update alongside version)

source:
  url: https://github.com/amirhosseindavoody/csv-utils/archive/${{ tag }}.tar.gz
  sha256: f911511a52d195a5330d78502ce663d340ad8a9fe8e3faa83cb40cfc89081769
```

**Longer term (optional, discuss with reviewer):**

| Option | Pros | Cons |
|--------|------|------|
| Keep `2026.6.24+N` | Matches Cargo/Pixi | Manual feedstock bumps; bot unlikely to work |
| Use dotted conda version `2026.6.24.2` | Conda-friendly | Diverges from Cargo `+N` encoding |
| Add semver GitHub releases (`v0.1.0`) for conda | Bot-friendly | Two version schemes to manage |

For the PR reply: we'll template the URL and note that autotick may need manual bumps unless versioning changes.

---

## Comment 4 — Remove `.crates.toml` cleanup

**Location:** [`recipes/csv-utils/build.sh`](https://github.com/conda-forge/staged-recipes/pull/33899#discussion_r3479724282) — last lines (same applies to `build.bat`)  
**Review link:** [discussion_r3479724282](https://github.com/conda-forge/staged-recipes/pull/33899#discussion_r3479724282)

### What you have now

```bash
cargo install --no-track --locked --root "$PREFIX" --path csv-utils
cargo install --no-track --locked --root "$PREFIX" --path csv-utils-web

cargo-bundle-licenses --format yaml --output THIRDPARTY.yml

rm -f "$PREFIX/.crates.toml" "$PREFIX/.crates2.json"   # ← reviewer says remove
```

### What the reviewer is asking

Delete the `rm` / `del` lines — they're redundant with `--no-track`.

### Why this matters

By default, `cargo install` writes **`$PREFIX/.crates.toml`** and **`.crates2.json`** — a registry of installed crates. Conda packages shouldn't ship those; they're an implementation detail of cargo's install tracking.

**`--no-track`** tells cargo **not to create those files**. Cleaning them up afterward is defensive but unnecessary — and the [official template](https://conda-forge.org/docs/maintainer/example_recipes/rust/) doesn't include that step.

With **`strict: true`** (Comment 1), leaving redundant cleanup also adds noise to the recipe.

### What we plan to change

Remove from both Unix and Windows scripts:

```bash
# DELETE these lines
rm -f "$PREFIX/.crates.toml" "$PREFIX/.crates2.json"
```

```bat
REM DELETE these lines
del /q "%PREFIX%\.crates.toml" 2>nul
del /q "%PREFIX%\.crates2.json" 2>nul
```

---

## Comment 5 — Align with the official Rust recipe template

**Location:** entire [`recipes/csv-utils/recipe.yaml`](https://github.com/conda-forge/staged-recipes/pull/33899#discussion_r3479726267)  
**Review link:** [discussion_r3479726267](https://github.com/conda-forge/staged-recipes/pull/33899#discussion_r3479726267)

### What the reviewer is asking

Restructure the recipe to follow [conda-forge.org/docs/maintainer/example_recipes/rust/](https://conda-forge.org/docs/maintainer/example_recipes/rust/) as closely as practical.

### Why this matters

conda-forge is maintained by volunteers. **Consistent recipes** are faster to review, easier to copy from, and less likely to miss platform-specific details (e.g. Windows `LIBRARY_PREFIX` vs Unix `PREFIX`).

### Gaps vs the template today

| Template convention | Your PR now | Planned |
|---------------------|-------------|---------|
| Inline `build.script` in `recipe.yaml` | Separate `build.sh` / `build.bat` | Move to inline script (or keep scripts only if needed) |
| `cargo auditable install` | `cargo install` | Switch (Comment 2) |
| `CARGO_PROFILE_RELEASE_LTO: fat` | Only `STRIP=symbols` | Add LTO env var |
| Windows install root | `%PREFIX%` | `%LIBRARY_PREFIX%` |
| `package_contents` + `strict: true` | Missing | Add (Comment 1) |
| Shell completions | Optional | Skip (not supported by `csv` today) |
| `--no-track` without manual `rm` | Extra cleanup | Remove (Comment 4) |

### Planned recipe shape (summary)

```yaml
context:
  version: "2026.6.24+2"
  tag: "v2026.6.24%2B2"

package:
  name: csv-utils
  version: ${{ version }}

source:
  url: https://github.com/amirhosseindavoody/csv-utils/archive/${{ tag }}.tar.gz
  sha256: f911511a52d195a5330d78502ce663d340ad8a9fe8e3faa83cb40cfc89081769

build:
  number: 0
  script:
    env:
      CARGO_PROFILE_RELEASE_STRIP: symbols
      CARGO_PROFILE_RELEASE_LTO: fat
    content:
      - if: unix
        then:
          - cargo auditable install --locked --no-track --bins --root ${{ PREFIX }} --path csv-utils
          - cargo auditable install --locked --no-track --bins --root ${{ PREFIX }} --path csv-utils-web
        else:
          - cargo auditable install --locked --no-track --bins --root %LIBRARY_PREFIX% --path csv-utils
          - cargo auditable install --locked --no-track --bins --root %LIBRARY_PREFIX% --path csv-utils-web
      - cargo-bundle-licenses --format yaml --output ./THIRDPARTY.yml

requirements:
  build:
    - ${{ stdlib('c') }}
    - ${{ compiler('c') }}
    - ${{ compiler('rust') }}
    - cargo-bundle-licenses
    - cargo-auditable

tests:
  - script:
      - csv --help
      - csv-utils-web --help
  - package_contents:
      bin:
        - csv
        - csv-utils-web
      strict: true

about:
  # ... unchanged ...
```

Also **delete** `build.sh` and `build.bat` if everything moves inline (matches the template).

---

## Summary checklist

| # | Comment | File | Action |
|---|---------|------|--------|
| 1 | `package_contents` + `strict: true` | `recipe.yaml` | Add binary presence + strict packaging test |
| 2 | Use `cargo-auditable` | `recipe.yaml` + build | Add dep; use `cargo auditable install` |
| 3 | Template source URL | `recipe.yaml` | `${{ tag }}` variable; discuss versioning / autotick |
| 4 | Remove `.crates` cleanup | `build.sh` / `build.bat` | Delete redundant `rm`/`del` lines |
| 5 | Follow Rust template | whole recipe | Inline script, LTO, `LIBRARY_PREFIX` on Windows, etc. |

---

## Suggested PR reply (when you push fixes)

> Thanks @pavelzw! Updated the recipe to follow the [Rust template](https://conda-forge.org/docs/maintainer/example_recipes/rust/): `cargo auditable install`, `package_contents` with `strict: true`, templated source URL via a `tag` context var (needed because `+` must be `%2B` in GitHub archive URLs). Removed the `.crates.toml` cleanup since we use `--no-track`. Skipped shell-completion install/tests since `csv` doesn't support `completion --shell` yet. On versioning: we'll keep calver+build metadata for now and bump the feedstock manually unless we switch to bot-friendly tags later.
