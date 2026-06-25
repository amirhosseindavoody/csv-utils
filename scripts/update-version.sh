#!/usr/bin/env bash
# Bump project version (Cargo semver) in Cargo.toml and pixi.toml.
#
# Semver stored in files:  YYYY.M.D+N   (e.g. 2026.6.24+0)
# Human / calver meaning: YYYY.MM.DD.N (e.g. 2026.06.24.0)
#
# N is 0 on the first bump of a calendar day; further runs the same day increment N.
# Cargo requires semver; a fourth dotted segment (2026.06.24.0) is invalid.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CARGO_TOML="$ROOT/Cargo.toml"
PIXI_TOML="$ROOT/pixi.toml"
RECIPE_YAML="$ROOT/recipe/recipe.yaml"

today_semver() {
  local year month day
  year="$(date +%Y)"
  month=$((10#$(date +%m)))
  day=$((10#$(date +%d)))
  echo "${year}.${month}.${day}"
}

today_human() {
  date +%Y.%m.%d
}

human_label() {
  local version="$1"
  if [[ "$version" =~ ^([0-9]{4})\.([0-9]{1,2})\.([0-9]{1,2})\+([0-9]+)$ ]]; then
    printf "%04d.%02d.%02d.%s" "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}" "${BASH_REMATCH[3]}" "${BASH_REMATCH[4]}"
    return
  fi
  if [[ "$version" =~ ^([0-9]{4})\.([0-9]{2})\.([0-9]{2})\.([0-9]+)$ ]]; then
    echo "$version"
    return
  fi
  echo "$version"
}

read_workspace_version() {
  awk '
    /^\[workspace\.package\]$/ { in_sec = 1; next }
    /^\[/ { in_sec = 0 }
    in_sec && /^version =/ {
      line = $0
      sub(/^version = "/, "", line)
      sub(/"$/, "", line)
      print line
      exit
    }
  ' "$CARGO_TOML"
}

next_version() {
  local current="$1"
  local prefix="${2:?}"
  local build=0

  if [[ "$current" =~ ^([0-9]{4})\.([0-9]{1,2})\.([0-9]{1,2})\+([0-9]+)$ ]]; then
    local date_part="${BASH_REMATCH[1]}.${BASH_REMATCH[2]}.${BASH_REMATCH[3]}"
    build="${BASH_REMATCH[4]}"
    if [[ "$date_part" == "$prefix" ]]; then
      echo "${prefix}+$((build + 1))"
      return
    fi
  fi

  # Legacy invalid calver written before semver fix (YYYY.MM.DD.N).
  if [[ "$current" =~ ^([0-9]{4})\.([0-9]{2})\.([0-9]{2})\.([0-9]+)$ ]]; then
    local year="${BASH_REMATCH[1]}"
    local month=$((10#${BASH_REMATCH[2]}))
    local day=$((10#${BASH_REMATCH[3]}))
    build="${BASH_REMATCH[4]}"
    local legacy_semver="${year}.${month}.${day}"
    if [[ "$legacy_semver" == "$prefix" ]]; then
      echo "${prefix}+$((build + 1))"
      return
    fi
  fi

  echo "${prefix}+0"
}

update_cargo_version() {
  local version="$1"
  awk -v ver="$version" '
    /^\[workspace\.package\]$/ { in_sec = 1; print; next }
    /^\[/ { in_sec = 0 }
    in_sec && /^version =/ {
      print "version = \"" ver "\""
      next
    }
    { print }
  ' "$CARGO_TOML"
}

update_pixi_version() {
  local version="$1"
  awk -v ver="$version" '
    /^\[workspace\]$/ { in_ws = 1; in_pkg = 0; print; next }
    /^\[package\]$/ { in_pkg = 1; in_ws = 0; print; next }
    /^\[/ { in_ws = 0; in_pkg = 0 }
    (in_ws || in_pkg) && /^version =/ {
      print "version = \"" ver "\""
      next
    }
    { print }
  ' "$PIXI_TOML"
}

update_recipe_version() {
  local version="$1"
  awk -v ver="$version" '
    /^context:$/ { in_ctx = 1; print; next }
    /^[^ #]/ { in_ctx = 0 }
    in_ctx && /^  version:/ {
      print "  version: " ver
      next
    }
    { print }
  ' "$RECIPE_YAML"
}

main() {
  if [[ ! -f "$CARGO_TOML" || ! -f "$PIXI_TOML" ]]; then
    echo "update-version: expected Cargo.toml and pixi.toml in $ROOT" >&2
    exit 1
  fi

  local prefix current new_version
  prefix="$(today_semver)"
  current="$(read_workspace_version || true)"
  new_version="$(next_version "$current" "$prefix")"

  local cargo_tmp pixi_tmp recipe_tmp
  cargo_tmp="$(mktemp)"
  pixi_tmp="$(mktemp)"
  recipe_tmp="$(mktemp)"
  trap 'rm -f "$cargo_tmp" "$pixi_tmp" "$recipe_tmp"' EXIT

  update_cargo_version "$new_version" >"$cargo_tmp"
  update_pixi_version "$new_version" >"$pixi_tmp"
  mv "$cargo_tmp" "$CARGO_TOML"
  mv "$pixi_tmp" "$PIXI_TOML"
  if [[ -f "$RECIPE_YAML" ]]; then
    update_recipe_version "$new_version" >"$recipe_tmp"
    mv "$recipe_tmp" "$RECIPE_YAML"
  fi
  trap - EXIT

  local current_label new_label
  current_label="$(human_label "$current")"
  new_label="$(human_label "$new_version")"

  if [[ "$current" == "$new_version" ]]; then
    echo "Version unchanged: ${new_label} (${new_version})"
  else
    echo "Version: ${current_label:-(unset)} (${current:-unset}) -> ${new_label} (${new_version})"
  fi
}

main "$@"
