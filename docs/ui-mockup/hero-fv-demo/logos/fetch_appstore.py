#!/usr/bin/env python3
"""Fetch the real app artwork used in the demo Dock, and prepare it for embedding.

Source is Apple's public App Store search endpoint (no key, no auth). Each entry below is
pinned by an exact (trackName, sellerName) pair rather than "first search hit", so a re-run
either returns the same artwork or fails loudly — it never silently swaps in a lookalike from
some other publisher.

    logos/appstore/app_<key>.png   the 512x512 original, as served
    logos/appstore/128/<key>.png   what actually ships: 128px, squircle-masked

Run from docs/ui-mockup/hero-fv-demo/ , then `python3 build.py`.
"""
import json, os, ssl, urllib.parse, urllib.request
from PIL import Image

# key            entity         trackName                        sellerName
SPEC = [
    ("safari",    "software",    "Safari",                        "Apple Inc."),
    ("messages",  "software",    "Messages",                      "Apple Inc."),
    ("mail",      "software",    "Mail",                          "Apple Inc."),
    ("maps",      "software",    "Apple Maps",                    "Apple Inc."),
    ("photos",    "software",    "Photos",                        "Apple Inc."),
    ("facetime",  "software",    "FaceTime",                      "Apple Inc."),
    ("calendar",  "software",    "Calendar",                      "Apple Inc."),
    ("contacts",  "software",    "Contacts",                      "Apple Inc."),
    ("reminders", "software",    "Reminders",                     "Apple Inc."),
    ("notes",     "software",    "Notes",                         "Apple Inc."),
    ("music",     "software",    "Apple Music",                   "Apple Inc."),
    ("freeform",  "software",    "Freeform",                      "Apple Inc."),
    ("keynote",   "macSoftware", "Keynote: Design Presentations", "Apple Inc."),
    ("numbers",   "macSoftware", "Numbers: Make Spreadsheets",    "Apple Inc."),
    ("pages",     "macSoftware", "Pages: Create Documents",       "Apple Inc."),
    ("slack",     "macSoftware", "Slack for Desktop",             "SLACK TECHNOLOGIES L.L.C."),
    ("line",      "macSoftware", "LINE",                          "LY Corporation"),
    ("notion",    "software",    "Notion: Notes, Tasks, AI",      "Notion Labs, Incorporated"),
    ("discord",   "software",    "Discord - Talk, Play, Hang Out","Discord Inc."),
    ("raycast",   "software",    "Raycast: AI, Notes and more",   "Raycast Technologies Inc"),
]

RAW, OUT, SIZE, SS = "logos/appstore", "logos/appstore/128", 128, 4
CTX = ssl.create_default_context(cafile="/root/.ccr/ca-bundle.crt") \
    if os.path.exists("/root/.ccr/ca-bundle.crt") else None


def squircle(side, n=5.0):
    """The macOS icon silhouette: a superellipse, not a rounded rect. Drawn at 4x and
    downsampled, because a hard-edged mask on a 128px icon reads as a jaggy cutout."""
    S = side * SS
    m = Image.new("L", (S, S), 0)
    px, a = m.load(), side * SS / 2.0
    for y in range(S):
        ny = abs((y + 0.5 - a) / a) ** n
        if ny > 1:
            continue
        half = a * (1 - ny) ** (1 / n)
        for x in range(max(0, int(round(a - half))), min(S, int(round(a + half)))):
            px[x, y] = 255
    return m.resize((side, side), Image.LANCZOS)


def fetch(term, entity, track, seller):
    u = ("https://itunes.apple.com/search?term=%s&entity=%s&country=us&limit=40"
         % (urllib.parse.quote(term), entity))
    with urllib.request.urlopen(u, timeout=30, context=CTX) as r:
        results = json.load(r)["results"]
    hit = next((x for x in results
                if x.get("trackName") == track and x.get("sellerName") == seller), None)
    if hit is None:
        raise SystemExit("no exact match for %r by %r — refusing to guess" % (track, seller))
    art = hit["artworkUrl512"]
    with urllib.request.urlopen(art, timeout=30, context=CTX) as r:
        return r.read()


def main():
    os.makedirs(OUT, exist_ok=True)
    mask = squircle(SIZE)
    for key, entity, track, seller in SPEC:
        raw = os.path.join(RAW, "app_%s.png" % key)
        if not os.path.exists(raw):
            open(raw, "wb").write(fetch(track.split(":")[0].split(" - ")[0], entity, track, seller))
        im = Image.open(raw)
        if im.mode == "RGBA" and im.getchannel("A").getbbox() != (0, 0, im.width, im.height):
            # The artwork already carries its own silhouette (LINE, Slack). Masking it again
            # would clip a shape that is already correct, so crop to the alpha instead.
            im, how = im.crop(im.getchannel("A").getbbox()).resize((SIZE, SIZE), Image.LANCZOS), "crop"
        else:
            im = im.convert("RGB").resize((SIZE, SIZE), Image.LANCZOS).convert("RGBA")
            im.putalpha(mask)
            how = "squircle"
        im.save(os.path.join(OUT, key + ".png"), optimize=True)
        print("%-10s %-9s %s" % (key, how, track))


if __name__ == "__main__":
    main()
