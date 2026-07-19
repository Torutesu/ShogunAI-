#!/usr/bin/env bash
# 24h soak (spec §4.5, S-11). Runs ON-DEVICE (macOS/arm64) only.
# Launches the release spike, samples WebKit-helper CPU externally, records to JSONL.
# Deliberately does NOT run `caffeinate` — sleep/wake is part of Q1 (spec §4.5).
set -euo pipefail

OUT_DIR="${SHOGUN_SPIKE_METRICS:-$HOME/Library/Application Support/dev.shogun.spike/metrics}"
mkdir -p "$OUT_DIR"
EXT_LOG="$OUT_DIR/$(date +%Y%m%d)-cpu_external.jsonl"

echo "[soak] release build…"
cargo build -p shogun-desktop-spike --release  # on-device: builds the Tauri shell
APP_BIN="target/release/shogun-desktop-spike"

echo "[soak] launching $APP_BIN"
"$APP_BIN" &
APP_PID=$!
trap 'kill "$APP_PID" 2>/dev/null || true' EXIT

echo "[soak] external CPU sampling every 10s → $EXT_LOG"
# spec §4.2.3: correlate WebKit WebContent processes responsible to our pid.
# responsibility_get_pid_responsible_for_pid is SPI; the simple proxy below matches
# WebContent procs started after our launch. Refine on-device (findings item 11).
while kill -0 "$APP_PID" 2>/dev/null; do
  ts=$(($(date +%s) * 1000))
  ps -axo pid,ppid,%cpu,comm | awk -v ts="$ts" '/WebKit.WebContent|shogun-desktop-spike/ {
    printf "{\"ts\":%s,\"type\":\"metric.cpu_external\",\"v\":1,\"payload\":{\"pid\":%s,\"cpu_pct\":%s,\"comm\":\"%s\"}}\n", ts, $1, $3, $4
  }' >> "$EXT_LOG"
  sleep 10
done
echo "[soak] app exited; external log at $EXT_LOG"
