#!/usr/bin/env bash
# Automated cache test, 100 updates (spec S-13). ON-DEVICE (macOS) only.
# Round-robins `activate` across 10 apps at 3s intervals so the app records
# metric.cache_update on each focus switch.
set -euo pipefail

APPS=(Safari Notes Mail Finder Terminal "Visual Studio Code" Preview Calendar Music System\ Settings)
ROUNDS="${1:-10}"

echo "[cache] ${#APPS[@]} apps × $ROUNDS rounds"
for _ in $(seq 1 "$ROUNDS"); do
  for app in "${APPS[@]}"; do
    osascript -e "tell application \"$app\" to activate" 2>/dev/null || \
      echo "[cache] skip (not installed): $app" >&2
    sleep 3
  done
done
echo "[cache] done; metric.cache_update recorded by the harness"
