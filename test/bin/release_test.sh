#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
RELEASE_SCRIPT="$REPO_ROOT/bin/dev/release.sh"
TEST_DATE="2026-04-14"

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  exit 1
}

minor_for_date() {
  local epoch_2026 epoch_target
  epoch_2026=$(date -j -f "%Y-%m-%d" "2026-01-01" "+%s")
  epoch_target=$(date -j -f "%Y-%m-%d" "$1" "+%s")
  printf '%s\n' $((((epoch_target - epoch_2026) / 86400) + 100))
}

setup_repo() {
  local repo
  repo="$(mktemp -d)"
  git init -b main "$repo" >/dev/null
  git -C "$repo" config user.name "Test User"
  git -C "$repo" config user.email "test@example.com"
  cat >"$repo/Cargo.toml" <<'EOF'
[workspace]
members = []

[workspace.package]
version = "0.176.2"
EOF
  git -C "$repo" add Cargo.toml
  git -C "$repo" commit -m "Initial commit" >/dev/null
  printf '%s\n' "$repo"
}

run_release() {
  local repo="$1"
  shift
  (
    cd "$repo"
    FABRO_RELEASE_DATE="$TEST_DATE" bash "$RELEASE_SCRIPT" "$@"
  )
}

test_stable_dry_run_is_side_effect_free() {
  local repo output expected_minor expected_tag
  repo="$(setup_repo)"
  expected_minor="$(minor_for_date "$TEST_DATE")"
  expected_tag="v0.${expected_minor}.0"

  if ! output="$(run_release "$repo" --dry-run 2>&1)"; then
    printf '%s\n' "$output" >&2
    fail "stable dry-run should succeed"
  fi

  [[ "$output" == *"tag ${expected_tag}"* ]] || fail "stable dry-run should print ${expected_tag}"
  git -C "$repo" diff --quiet || fail "stable dry-run should not modify tracked files"
  git -C "$repo" diff --cached --quiet || fail "stable dry-run should not stage changes"
}

test_prerelease_dry_run_increments_counter() {
  local repo output expected_minor expected_tag
  repo="$(setup_repo)"
  expected_minor="$(minor_for_date "$TEST_DATE")"

  git -C "$repo" tag -a "v0.${expected_minor}.0" -m "v0.${expected_minor}.0"
  git -C "$repo" tag -a "v0.${expected_minor}.1-alpha.0" -m "v0.${expected_minor}.1-alpha.0"
  expected_tag="v0.${expected_minor}.1-alpha.1"

  if ! output="$(run_release "$repo" alpha --dry-run 2>&1)"; then
    printf '%s\n' "$output" >&2
    fail "alpha dry-run should succeed"
  fi

  [[ "$output" == *"tag ${expected_tag}"* ]] || fail "alpha dry-run should print ${expected_tag}"
  git -C "$repo" diff --quiet || fail "alpha dry-run should not modify tracked files"
}

test_invalid_prerelease_label_is_rejected() {
  local repo output
  repo="$(setup_repo)"

  if output="$(run_release "$repo" foo --dry-run 2>&1)"; then
    printf '%s\n' "$output" >&2
    fail "invalid prerelease label should fail"
  fi

  [[ "$output" == *"invalid pre-release label"* ]] || fail "invalid label error should be clear"
}

test_stable_dry_run_is_side_effect_free
test_prerelease_dry_run_increments_counter
test_invalid_prerelease_label_is_rejected
