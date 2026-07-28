#!/usr/bin/env bash
# Fetch the on-device ASR model (MT3 meeting audio lane) for bundling or dev testing.
#
# The default is whisper small (ggml, multilingual, ~466MB) — deliberately NOT in git. This pulls
# it at setup time into a gitignored directory; packaging copies it into the .app as
# models/whisper-small.gguf. Without it the meeting lane degrades to notes-only (§7).
#
#   ./scripts/fetch-whisper-model.sh [dest] [variant]
#     variant: small (default) | large-v3-turbo
#
# Requires: curl. whisper.cpp/whisper-rs load ggml `.bin` weights (the .gguf name the app expects
# is just a path — a real ggml file works). Point the app at it with SHOGUN_WHISPER_MODEL.
set -euo pipefail

DEST="${1:-models/whisper}"
VARIANT="${2:-small}"
REPO="ggerganov/whisper.cpp"
BASE="https://huggingface.co/${REPO}/resolve/main"

case "$VARIANT" in
  small)          FILE="ggml-small.bin" ;;
  large-v3-turbo) FILE="ggml-large-v3-turbo.bin" ;;
  *) echo "unknown variant: $VARIANT (use: small | large-v3-turbo)"; exit 1 ;;
esac

mkdir -p "$DEST"

if [ -s "$DEST/$FILE" ]; then
  echo "already present: $DEST/$FILE"
else
  echo "fetching $FILE …"
  # -f so an HTML error page never lands on disk pretending to be a model.
  curl -fL --retry 3 --retry-delay 2 -o "$DEST/$FILE.part" "$BASE/$FILE"
  mv "$DEST/$FILE.part" "$DEST/$FILE"
fi

echo
echo "Model ready: $DEST/$FILE"
echo "Point the app at it for a dev run with:"
echo "  export SHOGUN_WHISPER_MODEL=$PWD/$DEST/$FILE"
