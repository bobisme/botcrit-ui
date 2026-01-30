#!/usr/bin/env bash
# Verify rendered theme colors against expected seed-derived values.
# Requires: botty, python3, cargo
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BINARY="$PROJECT_DIR/target/release/crit-ui"
THEMES_DIR="$PROJECT_DIR/themes"
AGENT_ID="crit-ui-verify"
SNAPSHOT_DIR=$(mktemp -d)
THRESHOLD=${THRESHOLD:-30}

cleanup() {
    botty kill "$AGENT_ID" 2>/dev/null || true
    rm -rf "$SNAPSHOT_DIR"
}
trap cleanup EXIT

echo "=== Theme Color Verification ==="
echo "Threshold: $THRESHOLD (max Euclidean distance per color)"
echo ""

# Step 1: Build
echo "Building release binary..."
cargo build --release --manifest-path="$PROJECT_DIR/Cargo.toml" 2>&1 | tail -1

# Step 2: Spawn and navigate to a review with diffs
echo "Spawning crit-ui..."
botty kill "$AGENT_ID" 2>/dev/null || true
sleep 1
botty spawn --name "$AGENT_ID" -- "$BINARY"
sleep 3

# Open first review
botty send "$AGENT_ID" "$(printf '\r')"
sleep 1

# Scroll down to see some diff content
botty send "$AGENT_ID" "jjjjjjjjjjjj"
sleep 0.5

# Step 3: For each theme, apply and snapshot
THEMES=(default-dark default-light catppuccin dracula gruvbox nord solarized monokai ayu vesper)

for theme_name in "${THEMES[@]}"; do
    echo -n "Testing $theme_name... "

    # Open theme picker via Ctrl+P -> "select"
    printf '\x10' | botty send-bytes "$AGENT_ID" "10"
    sleep 0.3
    botty send "$AGENT_ID" "select"
    sleep 0.5

    # Type theme name to filter
    botty send "$AGENT_ID" "$theme_name"
    sleep 0.3

    # Press Enter to apply
    botty send "$AGENT_ID" "$(printf '\r')"
    sleep 0.5

    # Take raw snapshot
    botty snapshot --raw "$AGENT_ID" > "$SNAPSHOT_DIR/${theme_name}.raw" 2>/dev/null

    echo "captured"
done

# Step 4: Analyze with Python
echo ""
echo "Analyzing snapshots..."
echo ""

python3 "$SCRIPT_DIR/analyze-theme-colors.py" \
    --themes-dir "$THEMES_DIR" \
    --snapshots-dir "$SNAPSHOT_DIR" \
    --threshold "$THRESHOLD"
