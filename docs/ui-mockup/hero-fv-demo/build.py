#!/usr/bin/env python3
"""Inline the wallpaper so the demo is one self-contained file.

    python3 build.py            # writes index.html next to this script

The published artifact must be a single file with no external requests, so the
photo goes in as a data URI rather than a sibling asset.
"""
import base64, pathlib

here = pathlib.Path(__file__).parent
src = (here / "index.src.html").read_text()
b64 = base64.b64encode((here / "wallpaper.jpg").read_bytes()).decode()
out = src.replace("__WALL__", "data:image/jpeg;base64," + b64)
(here / "index.html").write_text(out)
print(f"index.html written — {len(out)/1024/1024:.2f} MB")
