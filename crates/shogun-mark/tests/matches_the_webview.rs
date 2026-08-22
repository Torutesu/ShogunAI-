//! The mark exists twice, so something has to hold the two copies together.
//!
//! `apps/desktop/src/Logo.tsx` draws it for the app's windows and
//! `apps/desktop/src/styles/logo-motion.css` folds it there; this crate does both again for the
//! Dock and menu-bar icons, which AppKit draws and which never see a stylesheet. Neither copy can
//! import the other, so these tests read the TypeScript and the CSS as text and fail if the
//! numbers have parted company.
//!
//! When one of these fails, the fix is to bring the two into line — not to update the expectation.

use std::path::PathBuf;

use shogun_mark::geometry::{Facet, KABUTO};

fn repo_file(rel: &str) -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../..");
    path.push(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {} ({e}) — has it moved?", path.display()))
}

/// Pull the nested number array that follows `marker` out of a TypeScript source.
///
/// A depth counter rather than a line-by-line read, so reformatting the file does not break this.
fn nested_arrays(src: &str, marker: &str) -> Vec<Vec<[f32; 2]>> {
    let start = src
        .find(marker)
        .unwrap_or_else(|| panic!("`{marker}` is gone from Logo.tsx"));
    let body = &src[start + marker.len()..];

    let mut facets: Vec<Vec<[f32; 2]>> = Vec::new();
    let mut facet: Vec<[f32; 2]> = Vec::new();
    let mut numbers: Vec<f32> = Vec::new();
    let mut depth = 0_i32;
    let mut token = String::new();

    let flush = |token: &mut String, numbers: &mut Vec<f32>| {
        if !token.is_empty() {
            if let Ok(n) = token.parse::<f32>() {
                numbers.push(n);
            }
            token.clear();
        }
    };

    for ch in body.chars() {
        match ch {
            '[' => {
                depth += 1;
                if depth == 1 {
                    // the array of facets
                } else if depth == 2 {
                    facet = Vec::new();
                }
            }
            ']' => {
                flush(&mut token, &mut numbers);
                if depth == 3 {
                    assert_eq!(numbers.len(), 2, "a point with {} numbers", numbers.len());
                    facet.push([numbers[0], numbers[1]]);
                    numbers.clear();
                } else if depth == 2 {
                    facets.push(std::mem::take(&mut facet));
                } else if depth == 1 {
                    return facets;
                }
                depth -= 1;
            }
            ',' => flush(&mut token, &mut numbers),
            '-' | '.' | '0'..='9' => token.push(ch),
            _ => flush(&mut token, &mut numbers),
        }
    }
    panic!("`{marker}` never closed");
}

#[test]
fn the_vertices_are_the_ones_the_app_draws() {
    let src = repo_file("apps/desktop/src/Logo.tsx");
    let from_ts = nested_arrays(&src, "const KABUTO: readonly Polygon[] = ");

    assert_eq!(
        from_ts.len(),
        KABUTO.len(),
        "the webview draws {} facets, this crate has {}",
        from_ts.len(),
        KABUTO.len()
    );
    for (i, (ts, rust)) in from_ts.iter().zip(KABUTO.iter()).enumerate() {
        assert_eq!(
            ts.len(),
            rust.len(),
            "facet {i}: {} vertices in the webview, {} here",
            ts.len(),
            rust.len()
        );
        for (j, (a, b)) in ts.iter().zip(rust.iter()).enumerate() {
            assert_eq!(a, b, "facet {i} vertex {j} has drifted: webview {a:?}, here {b:?}");
        }
    }
}

#[test]
fn the_creases_are_the_ones_the_stylesheet_turns_about() {
    let css = repo_file("apps/desktop/src/styles/logo-motion.css");
    // The fold angles and the fill-box origins, exactly as geometry.rs holds them.
    for needle in [
        "transform-origin: 100% 50%",      // peak — the centre line
        "transform-origin: 80.94% 69.27%", // wing
        "transform-origin: 70.26% 20%",    // blade
        "rotate(-33.18deg)",
        "rotate(-63.43deg)",
    ] {
        assert!(css.contains(needle), "logo-motion.css no longer has `{needle}`");
    }
}

#[test]
fn the_arrival_keeps_the_stylesheet_time() {
    let css = repo_file("apps/desktop/src/styles/logo-motion.css");
    for needle in [
        "shogun-mark-in 760ms",              // the whole arrival
        "cubic-bezier(0.22, 1, 0.36, 1)",    // the ease every part settles on
        "animation-duration: 480ms",         // peak
        "animation-duration: 540ms",         // wing
        "animation-duration: 440ms",         // blade
        "animation-delay: 110ms",            // wing waits for the peak
        "animation-delay: 280ms",            // blade waits for the wing
    ] {
        assert!(css.contains(needle), "logo-motion.css no longer has `{needle}`");
    }
    assert_eq!(shogun_mark::DURATION_MS, 760.0, "the arrival changed length here but not there");
}

#[test]
fn the_facets_are_named_the_same_on_both_sides() {
    let src = repo_file("apps/desktop/src/Logo.tsx");
    assert!(
        src.contains(r#"const PARTS = ["peak", "wing", "blade"] as const;"#),
        "the webview renamed or reordered its facets; Facet::ALL has to follow"
    );
    assert_eq!(
        Facet::ALL.map(|f| format!("{f:?}").to_lowercase()),
        ["peak", "wing", "blade"].map(String::from)
    );
}
