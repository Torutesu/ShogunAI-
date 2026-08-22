//! The fold's last frame has to *be* the app icon.
//!
//! The Dock animation ends by handing the real bundle icon back, so if the frame before that
//! handoff sits a few pixels off, the launch ends with a visible jump. This redraws the shipped
//! icon from the same alpha the animation uses and compares it to the file that ships.
//!
//! The comparison deliberately skips every edge. Two rasterisers that were not the same program
//! will always disagree by a level or two along an anti-aliased boundary, and the shipped icon was
//! generated from the original artwork — which carries a 3px asymmetry in the wing that Logo.tsx
//! resolved in favour of true symmetry. What matters is the flat interior: if the mark were
//! misplaced, whole regions would come back the wrong colour, not a one-pixel rim.

use std::path::PathBuf;

/// The plate, measured off icons/icon-512.png: one flat fill under a squircle the artwork owns.
const PLATE: [u8; 3] = [215, 215, 215];
/// Brand blue, the same value Logo.tsx and the artwork use. The shipped PNG rounded it to
/// #004BFC on export — one level of green, which is inside the tolerance below and not worth
/// moving the brand value for.
const MARK: [u8; 3] = [0x00, 0x4c, 0xfc];
/// The mark spans 338 of the icon's 512 pixels, centred.
const PLACEMENT: shogun_mark::Placement = shogun_mark::Placement::new(338.0 / 512.0);

fn shipped_icon() -> image::RgbaImage {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../apps/desktop/src-tauri/icons/icon-512.png");
    let bytes =
        std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {} ({e})", path.display()));
    image::load_from_memory(&bytes)
        .unwrap_or_else(|e| panic!("icon-512.png will not decode: {e}"))
        .to_rgba8()
}

/// How far back from an edge a pixel has to sit to count as flat. Two, not one: the shipped icon's
/// own anti-aliasing runs about two pixels deep, so a one-pixel erosion still catches its rim and
/// reports it as a mismatch.
const ERODE: isize = 2;

/// How far a flat pixel may sit from its nominal colour. The shipped PNG is an export, not a
/// generated image: a few thousand of its pixels land two to five levels off the flat plate or the
/// flat blue. A mark drawn in the wrong place would be off by nearer two hundred, so this
/// separates "exported by a different tool" from "drawn somewhere else".
const TOLERANCE: i32 = 8;

/// True where this pixel and everything within [`ERODE`] of it agree on being wholly in or wholly
/// out — the flat inside of a facet, or the flat plate well clear of one.
fn deep(alpha: &[u8], w: usize, h: usize, x: usize, y: usize) -> Option<bool> {
    let (r, x, y) = (ERODE, x as isize, y as isize);
    if x < r || y < r || x + r >= w as isize || y + r >= h as isize {
        return None;
    }
    let (x, y) = (x as usize, y as usize);
    let here = alpha[y * w + x];
    if here != 0 && here != 255 {
        return None;
    }
    for dy in -r..=r {
        for dx in -r..=r {
            let nx = (x as isize + dx) as usize;
            let ny = (y as isize + dy) as usize;
            if alpha[ny * w + nx] != here {
                return None;
            }
        }
    }
    Some(here == 255)
}

#[test]
fn the_settled_frame_is_the_icon_that_ships() {
    let shipped = shipped_icon();
    let (w, h) = shipped.dimensions();
    assert_eq!(
        (w, h),
        (512, 512),
        "the icon changed size; the placement needs re-measuring"
    );

    let alpha = shogun_mark::still_alpha(w, h, PLACEMENT);
    let (w, h) = (w as usize, h as usize);

    let mut compared = 0_usize;
    let mut wrong = 0_usize;
    let mut worst = 0_i32;
    for y in 0..h {
        for x in 0..w {
            let Some(is_mark) = deep(&alpha, w, h, x, y) else {
                continue; // an edge, ours or the artwork's
            };
            let px = shipped.get_pixel(x as u32, y as u32);
            if px.0[3] < 255 {
                continue; // the plate's own anti-aliased rim
            }
            let want = if is_mark { MARK } else { PLATE };
            compared += 1;
            let off = (0..3)
                .map(|c| (px.0[c] as i32 - want[c] as i32).abs())
                .max()
                .unwrap_or(0);
            worst = worst.max(off);
            if off > TOLERANCE {
                wrong += 1;
            }
        }
    }

    assert!(
        compared > 100_000,
        "only {compared} pixels were flat enough to compare — the test stopped testing anything"
    );
    let share = wrong as f32 / compared as f32;
    assert!(
        share < 0.002,
        "{wrong} of {compared} flat pixels ({:.3}%) do not match the shipped icon (worst channel \
         off by {worst}) — the fold would end on a different picture than the one it hands back to",
        share * 100.0
    );
}
