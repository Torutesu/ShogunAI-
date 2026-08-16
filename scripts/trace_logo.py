#!/usr/bin/env python3
"""Trace logo.png into SVG assets and tray PNGs."""

from __future__ import annotations

import math
import subprocess
import xml.etree.ElementTree as ET
from pathlib import Path

import cv2
import numpy as np
from PIL import Image

REPO = Path(__file__).resolve().parents[1]
SOURCE = REPO / "logo.png"
FILL_HEX = "#004BFC"
FILL_RGB = (0, 75, 252)
NUM_SHARDS = 6


def load_blue_mask() -> tuple[np.ndarray, np.ndarray]:
    arr = np.array(Image.open(SOURCE).convert("RGBA"))
    blue = (arr[:, :, 2] > 200) & (arr[:, :, 0] < 30) & (arr[:, :, 1] < 120)
    return arr, blue


def segment_shards(blue: np.ndarray) -> list[np.ndarray]:
    ys, xs = np.where(blue)
    coords = np.column_stack([xs, ys]).astype(np.float32)
    criteria = (cv2.TERM_CRITERIA_EPS + cv2.TERM_CRITERIA_MAX_ITER, 100, 0.2)
    _, labels, _ = cv2.kmeans(
        coords, NUM_SHARDS, None, criteria, 10, cv2.KMEANS_PP_CENTERS
    )

    h, w = blue.shape
    label_img = np.zeros((h, w), np.int32)
    label_img[ys, xs] = labels.flatten() + 1

    blue_u8 = blue.astype(np.uint8) * 255
    shards: list[np.ndarray] = []

    for i in range(1, NUM_SHARDS + 1):
        region = cv2.bitwise_and((label_img == i).astype(np.uint8) * 255, blue_u8)
        cnts, _ = cv2.findContours(region, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE)
        cnt = max(cnts, key=cv2.contourArea)
        region_mask = np.zeros_like(blue_u8)
        cv2.drawContours(region_mask, [cnt], -1, 255, cv2.FILLED)

        best_pts: np.ndarray | None = None
        best_iou = -1.0
        peri = cv2.arcLength(cnt, True)
        for eps_frac in np.linspace(0.001, 0.03, 60):
            approx = cv2.approxPolyDP(cnt, float(eps_frac) * peri, True)
            if len(approx) < 4 or len(approx) > 10:
                continue
            poly = approx.reshape(-1, 2).astype(np.float64)
            poly = snap_vertices(poly, region_mask)
            test = np.zeros_like(blue_u8)
            cv2.fillPoly(test, [poly.astype(np.int32)], 255)
            shard_iou = iou_masks(region_mask > 0, test > 0)
            if shard_iou > best_iou:
                best_iou = shard_iou
                best_pts = poly

        if best_pts is None:
            approx = cv2.approxPolyDP(cnt, 0.008 * peri, True)
            best_pts = snap_vertices(approx.reshape(-1, 2).astype(np.float64), region_mask)

        shards.append(best_pts)

    return shards


def snap_vertices(poly: np.ndarray, region_mask: np.ndarray) -> np.ndarray:
    cnts, _ = cv2.findContours(region_mask, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_NONE)
    boundary = cnts[0].reshape(-1, 2).astype(np.float64)
    snapped = []
    for x, y in poly:
        dists = (boundary[:, 0] - x) ** 2 + (boundary[:, 1] - y) ** 2
        idx = int(np.argmin(dists))
        snapped.append(boundary[idx])
    return np.array(snapped, dtype=np.float64)


def iou_masks(a: np.ndarray, b: np.ndarray) -> float:
    inter = np.logical_and(a, b).sum()
    union = np.logical_or(a, b).sum()
    return float(inter / union) if union else 0.0


def bounds(shards: list[np.ndarray]) -> tuple[float, float, float, float]:
    all_pts = np.vstack(shards)
    min_x, min_y = all_pts.min(axis=0)
    max_x, max_y = all_pts.max(axis=0)
    return float(min_x), float(min_y), float(max_x), float(max_y)


def path_d(points: np.ndarray) -> str:
    pts = [(float(x), float(y)) for x, y in points]
    parts = [f"M {pts[0][0]:.2f} {pts[0][1]:.2f}"]
    for x, y in pts[1:]:
        parts.append(f"L {x:.2f} {y:.2f}")
    parts.append("Z")
    return " ".join(parts)


def write_color_svg(path: Path, shards: list[np.ndarray], view_box: tuple[float, float, float, float]) -> None:
    min_x, min_y, max_x, max_y = view_box
    width = max_x - min_x
    height = max_y - min_y
    svg_ns = "http://www.w3.org/2000/svg"
    root = ET.Element(
        "svg",
        {
            "xmlns": svg_ns,
            "viewBox": f"{min_x:.2f} {min_y:.2f} {width:.2f} {height:.2f}",
            "role": "img",
            "aria-label": "ShogunAI",
        },
    )
    for shard in shards:
        elem = ET.SubElement(
            root,
            "path",
            {"d": path_d(shard), "fill": FILL_HEX},
        )
    tree = ET.ElementTree(root)
    ET.indent(tree, space="  ")
    path.write_text(ET.tostring(root, encoding="unicode") + "\n", encoding="utf-8")


def write_template_svg(path: Path, shards: list[np.ndarray], square_size: float = 100.0, pad: float = 8.0) -> None:
    min_x, min_y, max_x, max_y = bounds(shards)
    logo_w = max_x - min_x
    logo_h = max_y - min_y
    inner = square_size - 2 * pad
    scale = min(inner / logo_w, inner / logo_h)
    tx = pad + (inner - logo_w * scale) / 2 - min_x * scale
    ty = pad + (inner - logo_h * scale) / 2 - min_y * scale

    svg_ns = "http://www.w3.org/2000/svg"
    root = ET.Element(
        "svg",
        {
            "xmlns": svg_ns,
            "viewBox": f"0 0 {square_size:.0f} {square_size:.0f}",
            "role": "img",
            "aria-label": "ShogunAI",
        },
    )
    group = ET.SubElement(
        root,
        "g",
        {"transform": f"translate({tx:.4f} {ty:.4f}) scale({scale:.6f})"},
    )
    for shard in shards:
        ET.SubElement(group, "path", {"d": path_d(shard), "fill": "#000000"})
    tree = ET.ElementTree(root)
    ET.indent(tree, space="  ")
    path.write_text(ET.tostring(root, encoding="unicode") + "\n", encoding="utf-8")


def rasterize_svg_to_png(svg_path: Path, png_path: Path, size: int) -> None:
    tmp_svg = png_path.with_suffix(".tmp.svg")
    tmp_svg.write_text(svg_path.read_text(encoding="utf-8"), encoding="utf-8")
    cmd = [
        "magick",
        "-background",
        "none",
        "-density",
        "512",
        tmp_svg.as_posix(),
        "-resize",
        f"{size}x{size}",
        png_path.as_posix(),
    ]
    subprocess.run(cmd, check=True)
    tmp_svg.unlink(missing_ok=True)


def render_polygons(shards: list[np.ndarray], shape: tuple[int, int]) -> np.ndarray:
    h, w = shape
    canvas = np.zeros((h, w), np.uint8)
    for shard in shards:
        cv2.fillPoly(canvas, [shard.astype(np.int32)], 255)
    return canvas


def verify_fidelity(blue: np.ndarray, shards: list[np.ndarray]) -> dict[str, float]:
    recon = render_polygons(shards, blue.shape) > 0
    iou = iou_masks(blue, recon)
    orig = blue.sum()
    inter = np.logical_and(blue, recon).sum()
    missed = orig - inter
    extra = recon.sum() - inter
    return {
        "iou": iou,
        "missed_px": float(missed),
        "extra_px": float(extra),
        "orig_px": float(orig),
    }


def main() -> None:
    _, blue = load_blue_mask()
    shards = segment_shards(blue)
    view_box = bounds(shards)

    root_svg = REPO / "logo.svg"
    mark_svg = REPO / "apps/desktop/src-tauri/icons/logo-mark.svg"
    template_svg = REPO / "apps/desktop/src-tauri/icons/logo-mark-template.svg"
    tray_1x = REPO / "apps/desktop/src-tauri/icons/tray-icon.png"
    tray_2x = REPO / "apps/desktop/src-tauri/icons/tray-icon@2x.png"

    write_color_svg(root_svg, shards, view_box)
    write_color_svg(mark_svg, shards, view_box)
    write_template_svg(template_svg, shards)

    rasterize_svg_to_png(template_svg, tray_1x, 22)
    rasterize_svg_to_png(template_svg, tray_2x, 44)

    metrics = verify_fidelity(blue, shards)
    path_count = len(shards)
    vertex_counts = [len(s) for s in shards]

    overlay_path = REPO / "scripts" / "logo_fidelity_overlay.png"
    overlay_path.parent.mkdir(parents=True, exist_ok=True)
    src_rgb = np.array(Image.open(SOURCE).convert("RGB"))
    recon_rgb = src_rgb.copy()
    recon = render_polygons(shards, blue.shape) > 0
    recon_rgb[~recon] = (recon_rgb[~recon] * 0.35).astype(np.uint8)
    recon_rgb[recon] = (recon_rgb[recon] * 0.65 + np.array(FILL_RGB) * 0.35).astype(np.uint8)
    Image.fromarray(recon_rgb).save(overlay_path)

    print("=== ShogunAI logo trace ===")
    print(f"Files written:")
    for p in [root_svg, mark_svg, template_svg, tray_1x, tray_2x, overlay_path]:
        print(f"  {p.relative_to(REPO)}")
    print(f"viewBox: {view_box[0]:.2f} {view_box[1]:.2f} {view_box[2]-view_box[0]:.2f} {view_box[3]-view_box[1]:.2f}")
    print(f"path count: {path_count}")
    print(f"vertices per path: {vertex_counts}")
    print(f"fill: {FILL_HEX}")
    print(f"IoU: {metrics['iou']*100:.3f}%")
    print(f"missed px: {metrics['missed_px']:.0f}, extra px: {metrics['extra_px']:.0f}")
    for name, p in [("tray-icon.png", tray_1x), ("tray-icon@2x.png", tray_2x)]:
        with Image.open(p) as im:
            print(f"{name}: {im.size[0]}x{im.size[1]}")


if __name__ == "__main__":
    main()
