#!/usr/bin/env python3
"""Inline the demo's binary assets so the published page is one self-contained file.

    python3 build.py            # writes index.html next to this script

The artifact host allows no external requests, so the wallpaper — and the dock
strip, when one is supplied — go in as data URIs rather than sibling files.

Drop a `dock.png` next to this script to replace the drawn dock with a real
screenshot of your own Dock. Capture it with Shift-Cmd-4, then Space, then click
the Dock: macOS writes a PNG with real transparency, which is what lets it sit
on the demo's wallpaper without carrying a rectangle of your own desktop with
it. Transparent margins are trimmed here, so the shadow the capture includes is
fine to leave in.
"""
import base64, pathlib, sys

here = pathlib.Path(__file__).parent
src = (here / "index.src.html").read_text()

def uri(path: pathlib.Path, mime: str) -> str:
    return f"data:{mime};base64," + base64.b64encode(path.read_bytes()).decode()

src = src.replace("__WALL__", uri(here / "wallpaper.jpg", "image/jpeg"))

dock = here / "dock.png"
if dock.exists():
    trimmed = dock
    try:
        from PIL import Image
        im = Image.open(dock).convert("RGBA")
        box = im.getchannel("A").getbbox()          # drop the shadow's empty margin
        if box:
            im = im.crop(box)
        trimmed = here / ".dock.trimmed.png"
        im.save(trimmed)
        print(f"dock.png  {im.width}x{im.height} after trim")
    except ImportError:
        print("dock.png used untrimmed (install pillow to trim its margins)", file=sys.stderr)
    src = src.replace("__DOCK__", uri(trimmed, "image/png"))
else:
    src = src.replace("__DOCK__", "")               # falls back to the drawn dock
    print("no dock.png — using the drawn dock")

(here / "index.html").write_text(src)
print(f"index.html written — {len(src)/1024/1024:.2f} MB")
