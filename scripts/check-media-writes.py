#!/usr/bin/env python3
"""CLAUDE.md invariant 2 guard: no image or audio bytes may be written to the filesystem.

The product's whole privacy claim rests on this: screen capture is Accessibility text, meetings
keep the transcript and not the waveform, and the two documented exceptions are narrow —
- OCR keyframes live as compressed JPEG in the ENCRYPTED memory DB for finite selected retention
  (`screen_frames`), never as files;
- 2026-08-05: meeting ASR streams audio to Deepgram for live transcription, process-only, and
  SHOGUN itself never writes the waveform anywhere.

Both exceptions are about a DB row and a socket. Neither permits a file. The failure mode this
catches is a plausible-looking one line — `fs::write(&path, frame.jpeg)?` in a debug helper, a
`NamedTempFile` to hand PCM to a decoder — that no reviewer would flag as a policy change.

Invariants 3 and 7 already have guards (check-http-egress.py, check-secret-exposure.py); this is
the third, so the rule that most defines the product is the one enforced least by hand.

Run from the repo root:  python3 scripts/check-media-writes.py
Self-test the detector:   python3 scripts/check-media-writes.py --self-test
"""

import pathlib
import re
import sys

# Filesystem write primitives. Anything that can put bytes on disk.
WRITE_RE = re.compile(
    r"""fs::write|File::create|OpenOptions|NamedTempFile|tempfile\s*\(|
        \.write_all\s*\(|BufWriter::new|create_new\s*\(\s*true""",
    re.VERBOSE,
)

# Media-bearing identifiers. Deliberately narrow: it is the CO-OCCURRENCE with a write primitive
# on the same line (or its immediate neighbours) that makes a hit, so these can be liberal
# without drowning the signal.
MEDIA_RE = re.compile(
    r"""jpeg|jpg|\bpng\b|\bwav\b|\bm4a\b|\bmp3\b|\bpcm\b|linear16|waveform|
        screenshot|screen_frame|frame_bytes|audio_bytes|samples_i16|Vec<i16>|Vec<f32>""",
    re.VERBOSE | re.IGNORECASE,
)

# How many lines around a write primitive count as "the same statement" — a multi-line call like
#   fs::write(
#       &path,
#       encode_frame_jpeg(&img)?,
#   )
# must still be caught.
WINDOW = 3

# Files permitted to pair the two. Each entry needs a reason; there are none today, and that is
# the point — the exceptions live in the DB and on a socket, not in the filesystem.
ALLOWLIST: set[str] = set()

SKIP_DIRS = {"target", "node_modules", ".git", "dist", ".next"}


def repo_rust_files():
    root = pathlib.Path(".")
    for path in sorted(root.rglob("*.rs")):
        if any(part in SKIP_DIRS for part in path.parts):
            continue
        rel = path.as_posix()
        try:
            yield rel, path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue


def scan(files):
    """Yield (path, lineno, line) where a filesystem write sits next to media bytes."""
    hits = []
    for path, text in files:
        if path in ALLOWLIST:
            continue
        lines = text.splitlines()
        for i, line in enumerate(lines):
            if not WRITE_RE.search(line):
                continue
            lo = max(0, i - WINDOW)
            hi = min(len(lines), i + WINDOW + 1)
            window = "\n".join(lines[lo:hi])
            # A comment-only mention (a doc line explaining the rule) is not a write.
            if MEDIA_RE.search(window) and not line.strip().startswith(("//", "///", "*")):
                hits.append((path, i + 1, line.strip()))
    return hits


def self_test():
    """The detector must catch a realistic violation and leave honest code alone."""
    bad = [
        (
            "crates/fake/src/leak.rs",
            'fn dump(path: &Path, img: &Image) {\n    let jpeg = encode_frame_jpeg(img);\n    fs::write(path, jpeg).ok();\n}',
        )
    ]
    assert scan(bad), "detector missed a jpeg written to disk"

    multiline = [
        (
            "crates/fake/src/leak2.rs",
            "fn dump(p: &Path) {\n    fs::write(\n        p,\n        encode_frame_jpeg(&img)?,\n    )?;\n}",
        )
    ]
    assert scan(multiline), "detector missed a multi-line media write"

    good = [
        # Text capture written to disk is fine — invariant 2 is about images and audio.
        ("crates/fake/src/ok.rs", 'fs::write(&path, transcript_text)?;'),
        # Media in memory, never written.
        ("crates/fake/src/ok2.rs", "let jpeg = encode_frame_jpeg(&img);\ndb.insert_screen_frame(&jpeg)?;"),
        # A comment that names the rule must not trip it.
        ("crates/fake/src/ok3.rs", "// never fs::write a jpeg or pcm buffer — invariant 2"),
    ]
    assert not scan(good), f"detector flagged honest code: {scan(good)}"

    allowed = [("crates/fake/src/leak.rs", 'fs::write(path, jpeg).ok();')]
    global ALLOWLIST
    ALLOWLIST = {"crates/fake/src/leak.rs"}
    assert not scan(allowed), "allowlist not honoured"
    ALLOWLIST = set()
    print("check-media-writes self-test OK")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return 0
    files = list(repo_rust_files())
    hits = scan(files)
    if hits:
        print("invariant-2 violation: image or audio bytes are being written to the filesystem.")
        for path, line, text in hits:
            print(f"  - {path}:{line}: {text}")
        print(
            "\nSHOGUN does not create screenshot, recording or audio files. OCR keyframes belong in "
            "the encrypted memory DB (`screen_frames`, finite age retention); meeting audio is process-only on its way "
            "to the ASR socket. If a site is genuinely unrelated, add it to ALLOWLIST here with a "
            "decision record."
        )
        return 1
    print(f"media writes OK: no filesystem write pairs with image/audio bytes ({len(files)} files scanned).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
