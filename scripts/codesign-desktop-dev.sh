#!/usr/bin/env bash
# Sign a desktop binary or app bundle with a stable identity so Accessibility, Microphone, and
# Keychain grants survive rebuilds. Ad-hoc signing changes the designated requirement whenever the
# executable changes, so it is intentionally not the default.
#
# One-time setup: add an Apple Development certificate in Xcode > Settings > Accounts. The script
# auto-detects the first valid code-signing identity, or accepts SHOGUN_SIGN_IDENTITY explicitly.
#
# Usage:
#   ./scripts/codesign-desktop-dev.sh
#   SHOGUN_SIGN_IDENTITY="Apple Development: Your Name (TEAMID)" ./scripts/codesign-desktop-dev.sh
#   SHOGUN_DESKTOP_TARGET=target/debug/bundle/macos/ShogunAI.app ./scripts/codesign-desktop-dev.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="${SHOGUN_DESKTOP_TARGET:-${SHOGUN_DESKTOP_BIN:-$ROOT/target/debug/shogun-desktop-spike}}"
ENT="$ROOT/apps/desktop/src-tauri/entitlements.plist"
IDENTITY="${SHOGUN_SIGN_IDENTITY:-}"

if [[ ! -e "$TARGET" ]]; then
  echo "Desktop target not found: $TARGET" >&2
  echo "Build first: cd apps/desktop && pnpm tauri build --debug" >&2
  exit 1
fi

if [[ ! -f "$ENT" ]]; then
  echo "Missing entitlements: $ENT" >&2
  exit 1
fi

if [[ -z "$IDENTITY" ]]; then
  IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null \
    | sed -nE 's/^[[:space:]]*[0-9]+\) [[:xdigit:]]+ "([^"]+)".*/\1/p' \
    | head -n 1)"
fi

if [[ -z "$IDENTITY" ]]; then
  echo "No valid code-signing identity found." >&2
  echo "Add an Apple Development certificate in Xcode > Settings > Accounts, then retry." >&2
  echo "Ad-hoc signing is unsafe for TCC persistence and is not used automatically." >&2
  exit 1
fi

if [[ "$IDENTITY" == "-" && "${SHOGUN_ALLOW_ADHOC:-0}" != "1" ]]; then
  echo "Refusing ad-hoc signing: Accessibility and Microphone grants would break after rebuild." >&2
  echo "For an explicit disposable build, also set SHOGUN_ALLOW_ADHOC=1." >&2
  exit 1
fi

codesign --force --sign "$IDENTITY" --entitlements "$ENT" --options runtime "$TARGET"
codesign --verify --strict "$TARGET"
echo "Signed $TARGET with stable identity: $IDENTITY"
