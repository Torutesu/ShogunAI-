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
#
# `-not -path '*.dSYM/*'` is load-bearing. The archive contains TWO files with this exact name:
#
#   lib/libonnxruntime.1.22.0.dylib                                        (33MB, the library)
#   lib/libonnxruntime.1.22.0.dylib.dSYM/Contents/Resources/DWARF/…same…   (71MB, debug symbols)
#
# `find … | head -1` picks whichever the filesystem hands back first, which differed between a
# local run and the CI runner. The runner got the dSYM, it signed and notarized without complaint,
# and the app died at dlopen with "unloadable mach-o file type 10" — MH_DSYM.
real="$(find "$tmp" -path '*/lib/*' -name "libonnxruntime.${VERSION}.dylib" -type f \
  -not -path '*.dSYM/*' | head -1)"
if [ -z "$real" ]; then
  echo "error: libonnxruntime.${VERSION}.dylib not found inside $ARCHIVE" >&2
  exit 1
fi
cp "$real" "$DEST/libonnxruntime.dylib"

# Assert what we actually copied. The Mach-O header's filetype field is at byte offset 12,
# little-endian: MH_DYLIB is 6, MH_DSYM is 10. Nothing downstream catches the difference —
# codesign signs a dSYM happily and notarization accepts it — so this is the only place the
# mistake can be caught before a user's Mac catches it.
filetype="$(od -An -t u4 -j 12 -N 4 "$DEST/libonnxruntime.dylib" | tr -d ' ')"
if [ "$filetype" != "6" ]; then
  echo "error: $DEST/libonnxruntime.dylib is not a loadable dylib (Mach-O filetype $filetype, want 6=MH_DYLIB)" >&2
  rm -f "$DEST/libonnxruntime.dylib"
  exit 1
fi

echo "ONNX Runtime ready: $DEST/libonnxruntime.dylib"
