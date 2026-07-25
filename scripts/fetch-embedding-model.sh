#!/usr/bin/env bash
# Fetch the local embedding model (ADR-001: multilingual-e5-small) for bundling into the app.
#
# The weights are ~470MB and deliberately NOT in git — this pulls them at build/setup time into a
# gitignored directory, and the packaging step copies them into the .app. Without them the product
# still runs; search simply stays lexical instead of hybrid.
#
#   ./scripts/fetch-embedding-model.sh [dest]
#
# Requires: curl. The ONNX Runtime shared library is fetched separately by the packaging step (the
# crate loads it dynamically) — see docs/embedding-model-setup.md.
set -euo pipefail

DEST="${1:-models/multilingual-e5-small}"
REPO="intfloat/multilingual-e5-small"
BASE="https://huggingface.co/${REPO}/resolve/main"

mkdir -p "$DEST"

fetch() {
  local name="$1" url="$2"
  if [ -s "$DEST/$name" ]; then
    echo "already present: $DEST/$name"
    return
  fi
  echo "fetching $name …"
  # -f so an HTML error page never lands on disk pretending to be a model.
  curl -fL --retry 3 --retry-delay 2 -o "$DEST/$name.part" "$url"
  mv "$DEST/$name.part" "$DEST/$name"
}

fetch "model.onnx" "$BASE/onnx/model.onnx"
fetch "tokenizer.json" "$BASE/tokenizer.json"

echo
echo "Model ready in $DEST"
echo "Point the app at it with:"
echo "  export SHOGUN_EMBED_MODEL=$PWD/$DEST/model.onnx"
echo "  export SHOGUN_EMBED_TOKENIZER=$PWD/$DEST/tokenizer.json"
