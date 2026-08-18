//! Text-region detection for the OCR gate — algorithm mirrored from Screenpipe
//! `screenpipe-screen/src/text_regions.rs` (#5054/#5060). Classical contour pipeline:
//! BT.601 grayscale → 3×3 morph gradient → Otsu → 9×1 close → connected components.
//! Screenpipe Commercial License prevents vendoring; implementation follows their
//! documented cv2-equivalent thresholds.
//! Reference: https://github.com/screenpipe/screenpipe

use image::DynamicImage;
use std::hash::{Hash, Hasher};

/// Detected text-like region in pixel coordinates of the input image.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

const MIN_BOX_W: u32 = 8;
const MIN_BOX_H: u32 = 6;
const MIN_ASPECT: f64 = 1.0;
const MAX_ASPECT: f64 = 40.0;
const MIN_AREA: u64 = 20;
const MAX_AREA_FRACTION: f64 = 0.5;

/// Detect text-like regions (geometry only — no character recognition).
pub fn detect_text_regions(image: &DynamicImage) -> Vec<TextRegion> {
    let (w, h) = (image.width() as usize, image.height() as usize);
    if w < 3 || h < 3 {
        return Vec::new();
    }

    let gray = to_gray_bt601(image);
    let mut gradient = morph_3x3::<true>(&gray, w, h);
    let eroded = morph_3x3::<false>(&gray, w, h);
    for (g, &e) in gradient.iter_mut().zip(&eroded) {
        *g -= e;
    }
    drop(eroded);

    let threshold = otsu_threshold(&gradient);
    let mut binary = vec![0u8; w * h];
    for i in 0..w * h {
        binary[i] = u8::from(gradient[i] > threshold);
    }
    drop(gradient);

    let closed = close_9x1(&binary, w, h);
    drop(binary);

    let boxes = connected_component_boxes(&closed, w, h);
    let total_area = (w as u64) * (h as u64);
    boxes
        .into_iter()
        .filter(|r| {
            if r.width < MIN_BOX_W || r.height < MIN_BOX_H {
                return false;
            }
            let aspect = r.width as f64 / r.height as f64;
            let area = r.width as u64 * r.height as u64;
            (MIN_ASPECT..=MAX_ASPECT).contains(&aspect)
                && area >= MIN_AREA
                && (area as f64) <= total_area as f64 * MAX_AREA_FRACTION
        })
        .collect()
}

/// Quantized luma hash — Screenpipe's OCR-gate skip signal (#5060).
pub fn image_pixel_signature(image: &DynamicImage) -> u64 {
    let gray = to_gray_bt601(image);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (image.width(), image.height()).hash(&mut hasher);
    let mut row_buf: Vec<u8> = Vec::with_capacity(image.width() as usize);
    for row in gray.chunks_exact(image.width().max(1) as usize) {
        row_buf.clear();
        row_buf.extend(row.iter().map(|&px| px >> 3));
        std::hash::Hasher::write(&mut hasher, &row_buf);
    }
    hasher.finish()
}

/// Union of regions, padded and clamped to the frame.
pub fn union_region(
    regions: &[TextRegion],
    pad: u32,
    frame_w: u32,
    frame_h: u32,
) -> Option<TextRegion> {
    let first = regions.first()?;
    let mut min_x = first.x;
    let mut min_y = first.y;
    let mut max_x = first.x + first.width;
    let mut max_y = first.y + first.height;
    for r in &regions[1..] {
        min_x = min_x.min(r.x);
        min_y = min_y.min(r.y);
        max_x = max_x.max(r.x + r.width);
        max_y = max_y.max(r.y + r.height);
    }
    let x = min_x.saturating_sub(pad);
    let y = min_y.saturating_sub(pad);
    let max_x = (max_x + pad).min(frame_w);
    let max_y = (max_y + pad).min(frame_h);
    if max_x <= x || max_y <= y {
        return None;
    }
    Some(TextRegion {
        x,
        y,
        width: max_x - x,
        height: max_y - y,
    })
}

fn to_gray_bt601(image: &DynamicImage) -> Vec<u8> {
    fn luma(r: u8, g: u8, b: u8) -> u8 {
        ((r as u32 * 4899 + g as u32 * 9617 + b as u32 * 1868 + 8192) >> 14) as u8
    }
    if let Some(rgba) = image.as_rgba8() {
        return rgba
            .chunks_exact(4)
            .map(|p| luma(p[0], p[1], p[2]))
            .collect();
    }
    if let Some(rgb) = image.as_rgb8() {
        return rgb
            .chunks_exact(3)
            .map(|p| luma(p[0], p[1], p[2]))
            .collect();
    }
    let rgb = image.to_rgb8();
    rgb.chunks_exact(3)
        .map(|p| luma(p[0], p[1], p[2]))
        .collect()
}

fn morph_3x3<const MAX: bool>(src: &[u8], w: usize, h: usize) -> Vec<u8> {
    #[inline(always)]
    fn op<const MAX: bool>(a: u8, b: u8) -> u8 {
        if MAX {
            a.max(b)
        } else {
            a.min(b)
        }
    }
    let mut horiz = vec![0u8; w * h];
    for y in 0..h {
        let row = &src[y * w..(y + 1) * w];
        let out = &mut horiz[y * w..(y + 1) * w];
        out[0] = op::<MAX>(row[0], row[1.min(w - 1)]);
        for x in 1..w - 1 {
            out[x] = op::<MAX>(op::<MAX>(row[x - 1], row[x]), row[x + 1]);
        }
        out[w - 1] = op::<MAX>(row[w - 2], row[w - 1]);
    }
    let mut out = vec![0u8; w * h];
    for y in 0..h {
        let lo = y.saturating_sub(1);
        let hi = (y + 1).min(h - 1);
        let dst = &mut out[y * w..(y + 1) * w];
        dst.copy_from_slice(&horiz[y * w..(y + 1) * w]);
        for yy in [lo, hi] {
            if yy == y {
                continue;
            }
            let src_row = &horiz[yy * w..(yy + 1) * w];
            for (d, &s) in dst.iter_mut().zip(src_row) {
                *d = op::<MAX>(*d, s);
            }
        }
    }
    out
}

fn otsu_threshold(pixels: &[u8]) -> u8 {
    let mut hist = [0u64; 256];
    for &p in pixels {
        hist[p as usize] += 1;
    }
    let total = pixels.len() as f64;
    let sum_all: f64 = hist
        .iter()
        .enumerate()
        .map(|(i, &c)| i as f64 * c as f64)
        .sum();
    let mut sum_bg = 0.0f64;
    let mut weight_bg = 0.0f64;
    let mut best_sigma = 0.0f64;
    let mut best_t = 0u8;
    for (t, &count) in hist.iter().enumerate() {
        weight_bg += count as f64;
        if weight_bg == 0.0 {
            continue;
        }
        let weight_fg = total - weight_bg;
        if weight_fg == 0.0 {
            break;
        }
        sum_bg += t as f64 * count as f64;
        let mean_bg = sum_bg / weight_bg;
        let mean_fg = (sum_all - sum_bg) / weight_fg;
        let sigma = weight_bg * weight_fg * (mean_bg - mean_fg) * (mean_bg - mean_fg);
        if sigma > best_sigma {
            best_sigma = sigma;
            best_t = t as u8;
        }
    }
    best_t
}

fn close_9x1(binary: &[u8], w: usize, h: usize) -> Vec<u8> {
    const R: usize = 4;
    let mut dilated = vec![0u8; w * h];
    for y in 0..h {
        let row = &binary[y * w..(y + 1) * w];
        let out = &mut dilated[y * w..(y + 1) * w];
        let mut count: u32 = 0;
        for &px in &row[..R.min(w)] {
            count += px as u32;
        }
        for x in 0..w {
            if x + R < w {
                count += row[x + R] as u32;
            }
            out[x] = u8::from(count > 0);
            if x >= R {
                count -= row[x - R] as u32;
            }
        }
    }
    let mut closed = vec![0u8; w * h];
    for y in 0..h {
        let row = &dilated[y * w..(y + 1) * w];
        let out = &mut closed[y * w..(y + 1) * w];
        let mut count: u32 = 0;
        for &px in &row[..R.min(w)] {
            count += px as u32;
        }
        for x in 0..w {
            if x + R < w {
                count += row[x + R] as u32;
            }
            let win = (x.min(R) + 1 + R.min(w - 1 - x)) as u32;
            out[x] = u8::from(count == win);
            if x >= R {
                count -= row[x - R] as u32;
            }
        }
    }
    closed
}

fn connected_component_boxes(binary: &[u8], w: usize, h: usize) -> Vec<TextRegion> {
    const NO_LABEL: u32 = u32::MAX;
    let mut labels = vec![NO_LABEL; w * h];
    let mut parent: Vec<u32> = Vec::new();

    fn find(parent: &mut [u32], mut i: u32) -> u32 {
        while parent[i as usize] != i {
            parent[i as usize] = parent[parent[i as usize] as usize];
            i = parent[i as usize];
        }
        i
    }
    fn union(parent: &mut [u32], a: u32, b: u32) {
        let (ra, rb) = (find(parent, a), find(parent, b));
        if ra != rb {
            parent[ra.max(rb) as usize] = ra.min(rb);
        }
    }

    for y in 0..h {
        for x in 0..w {
            if binary[y * w + x] == 0 {
                continue;
            }
            let mut neighbor_label = NO_LABEL;
            let mut consider = |lbl: u32, parent: &mut Vec<u32>| {
                if lbl != NO_LABEL {
                    if neighbor_label == NO_LABEL {
                        neighbor_label = lbl;
                    } else {
                        union(parent, neighbor_label, lbl);
                    }
                }
            };
            if x > 0 {
                consider(labels[y * w + x - 1], &mut parent);
            }
            if y > 0 {
                if x > 0 {
                    consider(labels[(y - 1) * w + x - 1], &mut parent);
                }
                consider(labels[(y - 1) * w + x], &mut parent);
                if x + 1 < w {
                    consider(labels[(y - 1) * w + x + 1], &mut parent);
                }
            }
            labels[y * w + x] = if neighbor_label == NO_LABEL {
                let new = parent.len() as u32;
                parent.push(new);
                new
            } else {
                neighbor_label
            };
        }
    }

    #[derive(Clone, Copy)]
    struct Extent {
        min_x: u32,
        min_y: u32,
        max_x: u32,
        max_y: u32,
    }
    let mut extents: Vec<Option<Extent>> = vec![None; parent.len()];
    for y in 0..h {
        for x in 0..w {
            let lbl = labels[y * w + x];
            if lbl == NO_LABEL {
                continue;
            }
            let root = find(&mut parent, lbl) as usize;
            let e = extents[root].get_or_insert(Extent {
                min_x: x as u32,
                min_y: y as u32,
                max_x: x as u32,
                max_y: y as u32,
            });
            e.min_x = e.min_x.min(x as u32);
            e.min_y = e.min_y.min(y as u32);
            e.max_x = e.max_x.max(x as u32);
            e.max_y = e.max_y.max(y as u32);
        }
    }

    extents
        .into_iter()
        .flatten()
        .map(|e| TextRegion {
            x: e.min_x,
            y: e.min_y,
            width: e.max_x - e.min_x + 1,
            height: e.max_y - e.min_y + 1,
        })
        .collect()
}
