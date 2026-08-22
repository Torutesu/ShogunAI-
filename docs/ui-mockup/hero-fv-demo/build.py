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

# Two builds from the one source, differing only in whether the finished loop is carried:
#
#   index.html           no video. This is the one that can be shared publicly.
#   index.download.html  the loop inlined, so the page can hand it to the viewer through the
#                        host's `downloads` capability — which the platform refuses to grant
#                        to a publicly-shared artifact. Publish this one privately.
#
# Base64 rather than a data: URI: the page decodes it to a Blob, and fetch() on a data URI is
# not something the artifact's CSP can be relied on to allow.
(here / "index.html").write_text(src.replace("__MP4__", ""))
print(f"index.html written — {len(src)/1024/1024:.2f} MB (no video; shareable)")

mp4 = here / "shogun-hero-mac.mp4"
if mp4.exists():
    full = src.replace("__MP4__", base64.b64encode(mp4.read_bytes()).decode())
    # Distinct title: the two builds otherwise sit side by side in the artifact gallery
    # under the same name, and only one of them can hand you the file.
    full = full.replace("<title>Notch Arrival</title>", "<title>Notch Arrival &mdash; MP4</title>")
    (here / "index.download.html").write_text(full)
    print(f"index.download.html written — {len(full)/1024/1024:.2f} MB "
          f"(carries {mp4.name}, {mp4.stat().st_size/1024/1024:.2f} MB)")
else:
    print("no shogun-hero-mac.mp4 — skipping index.download.html")
