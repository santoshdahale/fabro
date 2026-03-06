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
    local dot_dir
    dot_dir="$(dirname "$dot")"
    local dot_name
    dot_name="$(basename "$dot")"
    local rel
    rel="$(python3 -c "import os; print(os.path.relpath('$dot', '$SCRIPT_DIR'))")"

    # Check for companion run.toml (run-<stem>.toml in same dir)
    local stem
    stem="$(basename "${dot%.dot}")"
    local toml
    toml="${dot_dir}/run-${stem}.toml"

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
        preflight)
            # cd into the dot file's directory so relative paths resolve
            local target="$dot_name"
            [[ -f "$toml" ]] && target="run-${stem}.toml"

            if (cd "$dot_dir" && "$ARC" run start "$target" --preflight 2>&1); then
                echo "  PASS  $rel"
                pass=$((pass + 1))
            else
                echo "  FAIL  $rel"
                fail=$((fail + 1))
            fi
            ;;
        dry-run|haiku|full)
            # cd into the dot file's directory so relative script paths resolve
            local target="$dot_name"
            [[ -f "$toml" ]] && target="run-${stem}.toml"

            local flags=(--auto-approve)
            [[ "$PHASE" == "dry-run" ]] && flags+=(--dry-run)
            [[ "$PHASE" == "haiku" ]] && flags+=(--model claude-haiku-4-5)

            if (cd "$dot_dir" && "$ARC" run start "$target" "${flags[@]}" 2>&1); then
                echo "  PASS  $rel"
                pass=$((pass + 1))
            else
                echo "  FAIL  $rel"
                fail=$((fail + 1))
            fi
            ;;
        *)
            echo "Usage: $0 <validate|preflight|dry-run|haiku|full>"
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
