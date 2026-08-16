#!/usr/bin/env bash
# Fetch the ONNX Runtime shared library that gets bundled into the .app.
#
#   ./scripts/fetch-onnxruntime.sh [dest-dir]
#
# `ort` is built in `load-dynamic` mode: nothing links at build time and libonnxruntime is resolved
# by dlopen at first use. That keeps the build offline and lets the library ship inside the bundle —
# but it means SOMETHING has to put it there, and until now nothing did. A downloaded build searched
# only Homebrew and system prefixes (see crates/shogun-memory/src/embed_onnx.rs RUNTIME_DIRS), none
# of which exist on a normal user's Mac, so semantic search was off in every shipped build.
#
# The version is not free to choose: ort 2.0.0-rc.10 compiles against ORT_API_VERSION 22
# (ort-sys/src/lib.rs), which is ONNX Runtime 1.22.x. A runtime older than the headers returns a
# null OrtApi and the session fails to build.
#
# macOS arm64 only — the app's one supported target (CLAUDE.md).
set -euo pipefail

VERSION="${ONNXRUNTIME_VERSION:-1.22.0}"
DEST="${1:-apps/desktop/src-tauri/onnxruntime}"
ARCHIVE="onnxruntime-osx-arm64-${VERSION}.tgz"
URL="https://github.com/microsoft/onnxruntime/releases/download/v${VERSION}/${ARCHIVE}"

if [ -s "$DEST/libonnxruntime.dylib" ]; then
  echo "already present: $DEST/libonnxruntime.dylib"
  exit 0
fi

mkdir -p "$DEST"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "fetching ONNX Runtime ${VERSION} (arm64) …"
# -f so a GitHub error page never lands on disk pretending to be an archive.
curl -fL --retry 3 --retry-delay 2 -o "$tmp/$ARCHIVE" "$URL"
tar -xzf "$tmp/$ARCHIVE" -C "$tmp"

# The archive ships libonnxruntime.<version>.dylib plus a symlink. Copy the real file under the
# plain name the loader looks for: a symlink would dangle once the bundle is signed and moved.
real="$(find "$tmp" -name "libonnxruntime.${VERSION}.dylib" -type f | head -1)"
if [ -z "$real" ]; then
  echo "error: libonnxruntime.${VERSION}.dylib not found inside $ARCHIVE" >&2
  exit 1
fi
cp "$real" "$DEST/libonnxruntime.dylib"

echo "ONNX Runtime ready: $DEST/libonnxruntime.dylib"
