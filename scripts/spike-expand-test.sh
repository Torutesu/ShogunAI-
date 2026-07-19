#!/usr/bin/env bash
# Automated expand test, 200 iterations (spec S-12). ON-DEVICE (macOS/arm64) only.
# Injects: move to R_enter → 150ms dwell → move out → wait collapse, and records
# metric.expand_latency via the harness. Requires a mouse-injection helper.
#   - Preferred: `cliclick` (brew install cliclick), or a bundled CGEventPost helper.
#   - findings item 14: verify injected events reach the CGEventTap; if not, fall back
#     to 200 manual iterations (spec S-12).
set -euo pipefail

N="${1:-200}"
if ! command -v cliclick >/dev/null 2>&1; then
  echo "error: cliclick not found. brew install cliclick, or use the CGEventPost helper." >&2
  echo "       (spec S-12 fallback: run 200 manual iterations.)" >&2
  exit 3
fi

# R_enter centre is provided by the running app on stdout at startup (mode/notch geometry).
# On-device: read the notch centre from event.notch_geometry rather than hardcoding.
: "${RENTER_X:?set RENTER_X (notch centre x, px)}"
: "${RENTER_Y:?set RENTER_Y (near screen top, px)}"
PARK_X="${PARK_X:-$RENTER_X}"; PARK_Y="${PARK_Y:-600}"

echo "[expand] $N iterations at ($RENTER_X,$RENTER_Y)"
for i in $(seq 1 "$N"); do
  cliclick "m:$PARK_X,$PARK_Y"
  cliclick "m:$RENTER_X,$RENTER_Y"
  sleep 0.15          # dwell > 100ms → commit expand
  cliclick "m:$PARK_X,$PARK_Y"
  sleep 0.5           # allow collapse (grace 300ms + anim 160ms)
done
echo "[expand] done; metric.expand_latency recorded by the harness"
