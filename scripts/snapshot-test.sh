#!/usr/bin/env bash
# Snapshot tests for crit-ui views.
# Spawns crit-ui in demo mode at a fixed terminal size, navigates between
# views, captures plain-text snapshots, and diffs against committed baselines.
#
# Usage:
#   ./scripts/snapshot-test.sh                  # run tests (diff against baselines)
#   UPDATE_SNAPSHOTS=1 ./scripts/snapshot-test.sh   # regenerate baselines
#
# Requires: botty, cargo
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BINARY="$PROJECT_DIR/target/release/crit-ui"
SNAPSHOT_DIR="$PROJECT_DIR/tests/snapshots"
AGENT_ID="crit-ui-snap"
COLS=120
ROWS=40
FAILURES=0
UPDATE=${UPDATE_SNAPSHOTS:-0}

cleanup() {
    botty kill "$AGENT_ID" 2>/dev/null || true
}
trap cleanup EXIT

# ── Helpers ──────────────────────────────────────────────────────────────────

capture() {
    local name="$1"
    local actual
    actual=$(botty snapshot --raw "$AGENT_ID" 2>/dev/null)
    local baseline="$SNAPSHOT_DIR/$name"

    if [[ "$UPDATE" == "1" ]]; then
        echo "$actual" > "$baseline"
        echo "  updated $name"
        return
    fi

    if [[ ! -f "$baseline" ]]; then
        echo "  FAIL $name — baseline missing (run with UPDATE_SNAPSHOTS=1)"
        FAILURES=$((FAILURES + 1))
        return
    fi

    if ! diff -u "$baseline" <(echo "$actual") > /dev/null 2>&1; then
        echo "  FAIL $name"
        diff -u "$baseline" <(echo "$actual") || true
        FAILURES=$((FAILURES + 1))
    else
        echo "  ok   $name"
    fi
}

wait_for() {
    local text="$1"
    local timeout="${2:-10}"
    # Wait for text AND screen stability — avoids capturing mid-render frames
    botty wait "$AGENT_ID" --contains "$text" --stable 500 --timeout "$timeout" >/dev/null 2>&1
}

wait_stable() {
    local ms="${1:-500}"
    local timeout="${2:-10}"
    botty wait "$AGENT_ID" --stable "$ms" --timeout "$timeout" >/dev/null 2>&1
}

# ── Build ────────────────────────────────────────────────────────────────────

echo "=== Snapshot Tests ==="

if [[ ! -x "$BINARY" ]]; then
    echo "Building release binary..."
    cargo build --release --manifest-path="$PROJECT_DIR/Cargo.toml" 2>&1 | tail -1
fi

# ── Spawn ────────────────────────────────────────────────────────────────────

botty kill "$AGENT_ID" 2>/dev/null || true
sleep 1

echo "Spawning crit-ui (${COLS}x${ROWS})..."
botty spawn --name "$AGENT_ID" --no-resize --cols "$COLS" --rows "$ROWS" -- "$BINARY"

# Wait for review list to render
wait_for "Select"

# ── 01: Review List ──────────────────────────────────────────────────────────

echo "Capturing views..."
capture "01-review-list.txt"

# ── 02: Review Detail (Unified) ─────────────────────────────────────────────

# Press Enter to open the first review
botty send "$AGENT_ID" "$(printf '\r')"
# Wait for the diff content to actually render (jwt_secret is in the diff body)
wait_for "jwt_secret"

capture "02-review-detail-unified.txt"

# ── 03: Review Detail (Side-by-Side) ────────────────────────────────────────

# Press 'v' to toggle to side-by-side view
botty send "$AGENT_ID" "v"
wait_stable 500

capture "03-review-detail-sbs.txt"

# ── 04: Review Detail (Second File — Unified) ───────────────────────────────

# Press 'v' to go back to unified
botty send "$AGENT_ID" "v"
wait_stable 300

# Press ']' three times to navigate past threads to second file (src/main.rs)
# Sidebar: [0] src/auth.rs, [1] th-001, [2] th-002, [3] src/main.rs
botty send "$AGENT_ID" "]]]"
wait_stable 500

capture "04-review-detail-file2.txt"

# ── 05: Sidebar Focused ─────────────────────────────────────────────────────

# Press 'h' to toggle focus to sidebar
botty send "$AGENT_ID" "h"
wait_stable 300

capture "05-sidebar-focused.txt"

# ── Results ──────────────────────────────────────────────────────────────────

echo ""
if [[ "$UPDATE" == "1" ]]; then
    echo "Baselines updated in $SNAPSHOT_DIR"
    exit 0
fi

if [[ "$FAILURES" -gt 0 ]]; then
    echo "FAILED: $FAILURES snapshot(s) differ from baseline"
    exit 1
else
    echo "All snapshots match."
    exit 0
fi
