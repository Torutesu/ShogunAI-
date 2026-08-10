#!/usr/bin/env bash
# Sign the desktop debug binary with a stable local identity so Keychain "Always Allow" survives
# `cargo`/`tauri dev` rebuilds. What makes that work is the stable signature plus the open ACL
# items in keychain_store.rs — NOT the entitlements file, which is deliberately empty (see the
# comment in apps/desktop/src-tauri/entitlements.plist before adding anything to it).
#
# One-time setup (pick one):
#   A) Apple Development cert from Xcode (best): export SHOGUN_SIGN_IDENTITY="Apple Development: …"
#   B) Ad-hoc (weaker persistence on Sonoma+): SHOGUN_SIGN_IDENTITY="-"
#
# Usage:
#   ./scripts/codesign-desktop-dev.sh
#   SHOGUN_SIGN_IDENTITY="Apple Development: Your Name (TEAMID)" ./scripts/codesign-desktop-dev.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${SHOGUN_DESKTOP_BIN:-$ROOT/target/debug/shogun-desktop-spike}"
ENT="$ROOT/apps/desktop/src-tauri/entitlements.plist"
IDENTITY="${SHOGUN_SIGN_IDENTITY:--}"

if [[ ! -f "$BIN" ]]; then
  echo "Binary not found — build first: cd apps/desktop && pnpm tauri build --debug" >&2
  exit 1
fi

if [[ ! -f "$ENT" ]]; then
  echo "Missing entitlements: $ENT" >&2
  exit 1
fi

codesign --force --sign "$IDENTITY" --entitlements "$ENT" --options runtime "$BIN"
echo "Signed $BIN with identity: $IDENTITY"
