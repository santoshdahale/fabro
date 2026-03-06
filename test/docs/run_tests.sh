#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
ARC="${ARC:-$REPO_ROOT/target/release/arc}"

PHASE="${1:-validate}"

pass=0
fail=0
total=0

run_one() {
    local dot="$1"
    local rel
    rel="$(realpath --relative-to="$SCRIPT_DIR" "$dot" 2>/dev/null || echo "$dot")"

    # Check for companion run.toml (run-<stem>.toml in same dir)
    local stem
    stem="$(basename "${dot%.dot}")"
    local toml
    toml="$(dirname "$dot")/run-${stem}.toml"

    total=$((total + 1))

    case "$PHASE" in
        validate)
            if "$ARC" validate "$dot" 2>&1; then
                echo "  PASS  $rel"
                pass=$((pass + 1))
            else
                echo "  FAIL  $rel"
                fail=$((fail + 1))
            fi
            ;;
        dry-run)
            local target="$dot"
            [[ -f "$toml" ]] && target="$toml"
            if "$ARC" run start "$target" --dry-run --auto-approve 2>&1; then
                echo "  PASS  $rel"
                pass=$((pass + 1))
            else
                echo "  FAIL  $rel"
                fail=$((fail + 1))
            fi
            ;;
        haiku)
            local target="$dot"
            [[ -f "$toml" ]] && target="$toml"
            if "$ARC" run start "$target" --model claude-haiku-4-5 --auto-approve 2>&1; then
                echo "  PASS  $rel"
                pass=$((pass + 1))
            else
                echo "  FAIL  $rel"
                fail=$((fail + 1))
            fi
            ;;
        full)
            local target="$dot"
            [[ -f "$toml" ]] && target="$toml"
            if "$ARC" run start "$target" --auto-approve 2>&1; then
                echo "  PASS  $rel"
                pass=$((pass + 1))
            else
                echo "  FAIL  $rel"
                fail=$((fail + 1))
            fi
            ;;
        *)
            echo "Usage: $0 <validate|dry-run|haiku|full>"
            exit 1
            ;;
    esac
}

echo "=== Phase: $PHASE ==="
echo ""

while IFS= read -r dot; do
    run_one "$dot"
done < <(find "$SCRIPT_DIR" -name '*.dot' | sort)

echo ""
echo "=== Results: $pass passed, $fail failed, $total total ==="

[[ $fail -eq 0 ]]
