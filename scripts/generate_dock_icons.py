#!/usr/bin/env python3
"""Generate SHOGUN fan dock icon variants from logo.png traced geometry."""

from __future__ import annotations

import math
from collections import deque
from pathlib import Path

from PIL import Image, ImageDraw

REPO_ROOT = Path(__file__).resolve().parents[1]
BRAND_DIR = REPO_ROOT / "assets" / "brand"
SOURCE = BRAND_DIR / "logo.png"
CANVAS = 1024
MARGIN_RATIO = 0.15

BLUE = (0, 75, 252)
LIGHT_BG = (0xE8, 0xE8, 0xEC)
DARK_BG = (0x1A, 0x1A, 0x1A)
WHITE = (0xFF, 0xFF, 0xFF)
BLACK = (0x00, 0x00, 0x00)


def extract_polygons(source: Path) -> list[list[tuple[int, int]]]:
    img = Image.open(source)
    w, h = img.size
    pixels = img.load()

    mask = [[False] * w for _ in range(h)]
    for y in range(h):
        for x in range(w):
            r, g, b, a = pixels[x, y]
            if a > 128 and b > 100 and r < 50 and g < 120:
                mask[y][x] = True

    def flood(x: int, y: int) -> list[tuple[int, int]]:
        q = deque([(x, y)])
        comp: list[tuple[int, int]] = []
        while q:
            cx, cy = q.popleft()
            if cx < 0 or cy < 0 or cx >= w or cy >= h or not mask[cy][cx]:
                continue
            mask[cy][cx] = False
            comp.append((cx, cy))
            q.extend([(cx + 1, cy), (cx - 1, cy), (cx, cy + 1), (cx, cy - 1)])
        return comp

    def trace_boundary(comp: list[tuple[int, int]]) -> list[tuple[int, int]]:
        s = set(comp)
        cx = sum(p[0] for p in comp) / len(comp)
        cy = sum(p[1] for p in comp) / len(comp)
        bpts: list[tuple[int, int]] = []
        for x, y in comp:
            for dx, dy in (
                (1, 0),
                (-1, 0),
                (0, 1),
                (0, -1),
                (1, 1),
                (-1, 1),
                (-1, -1),
                (1, -1),
            ):
                if (x + dx, y + dy) not in s:
                    bpts.append((x, y))
                    break
        bpts = list(set(bpts))
        bpts.sort(key=lambda p: math.atan2(p[1] - cy, p[0] - cx))
        return bpts

    def rdp(points: list[tuple[int, int]], epsilon: float) -> list[tuple[int, int]]:
        if len(points) < 3:
            return points
        start, end = points[0], points[-1]
        sx, sy = start
        ex, ey = end
        max_dist = 0.0
        idx = 0
        for i in range(1, len(points) - 1):
            x, y = points[i]
            if ex == sx and ey == sy:
                dist = math.hypot(x - sx, y - sy)
            else:
                dist = abs((ey - sy) * x - (ex - sx) * y + ex * sy - ey * sx) / math.hypot(
                    ey - sy, ex - sx
                )
            if dist > max_dist:
                max_dist = dist
                idx = i
        if max_dist > epsilon:
            left = rdp(points[: idx + 1], epsilon)
            right = rdp(points[idx:], epsilon)
            return left[:-1] + right
        return [start, end]

    polygons: list[list[tuple[int, int]]] = []
    for y in range(h):
        for x in range(w):
            if mask[y][x]:
                comp = flood(x, y)
                if len(comp) > 100:
                    boundary = trace_boundary(comp)
                    simplified = rdp(boundary, 12.0)
                    polygons.append(simplified)

    return polygons


def transform_polygons(
    polygons: list[list[tuple[int, int]]],
    canvas: int,
    margin_ratio: float,
) -> list[list[tuple[float, float]]]:
    xs = [p[0] for poly in polygons for p in poly]
    ys = [p[1] for poly in polygons for p in poly]
    min_x, max_x = min(xs), max(xs)
    min_y, max_y = min(ys), max(ys)
    content_w = max_x - min_x
    content_h = max_y - min_y

    usable = canvas * (1.0 - 2.0 * margin_ratio)
    scale = min(usable / content_w, usable / content_h)
    offset_x = (canvas - content_w * scale) / 2.0
    offset_y = (canvas - content_h * scale) / 2.0

    transformed: list[list[tuple[float, float]]] = []
    for poly in polygons:
        transformed.append(
            [
                ((x - min_x) * scale + offset_x, (y - min_y) * scale + offset_y)
                for x, y in poly
            ]
        )
    return transformed


def render_icon(
    polygons: list[list[tuple[float, float]]],
    background: tuple[int, int, int] | None,
    fill: tuple[int, int, int],
    out_path: Path,
) -> None:
    if background is None:
        img = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
        draw = ImageDraw.Draw(img, "RGBA")
        for poly in polygons:
            draw.polygon(poly, fill=(*fill, 255))
        img.save(out_path, "PNG")
        return

    img = Image.new("RGB", (CANVAS, CANVAS), background)
    draw = ImageDraw.Draw(img)
    for poly in polygons:
        draw.polygon(poly, fill=fill)
    img.save(out_path, "PNG")


def main() -> None:
    polygons = extract_polygons(SOURCE)
    transformed = transform_polygons(polygons, CANVAS, MARGIN_RATIO)

    outputs = {
        "logo-transparent-blue.png": (None, BLUE),
        "logo-light-bg-blue.png": (LIGHT_BG, BLUE),
        "logo-dark-bg-white.png": (DARK_BG, WHITE),
        "logo-white-bg-black.png": (WHITE, BLACK),
    }

    for filename, (bg, fill) in outputs.items():
        render_icon(transformed, bg, fill, BRAND_DIR / filename)
        print(f"written {filename}")


if __name__ == "__main__":
    main()
