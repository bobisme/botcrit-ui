#!/usr/bin/env bash
# Wrapper for botcrit's demo generator.
#
# Usage:
#   ./scripts/generate-demo.sh          # Creates demo in /tmp/crit-demo-XXXXXX
#   ./scripts/generate-demo.sh /path    # Creates demo at custom path
#
# Set BOTCRIT_ROOT to override the botcrit repo location.

set -euo pipefail

BOTCRIT_ROOT="${BOTCRIT_ROOT:-$HOME/src/botcrit}"
SCRIPT="$BOTCRIT_ROOT/scripts/generate-demo.sh"

if [[ ! -x "$SCRIPT" ]]; then
	echo "generate-demo.sh not found at $SCRIPT." >&2
	echo "Set BOTCRIT_ROOT or clone botcrit to ~/src/botcrit." >&2
	exit 1
fi

exec "$SCRIPT" "$@"
