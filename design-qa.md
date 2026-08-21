# Design QA — Hero MacBook opening sequence

## Evidence

- Source product film: `/Users/torutano/Downloads/shogunheromac1200.mp4`
- Source opening sequence: `/tmp/shogun-hero-frames/line-composite.jpg`
- Closed-state implementation: `/tmp/shogun-hero-alpha-fix-early.png`
- Opening-state implementation: `/tmp/shogun-hero-alpha-fix-opening.png`
- Open-product implementation: `/tmp/shogun-hero-alpha-fix-open.png`
- Combined source/implementation review: `/tmp/shogun-hero-alpha-fix-comparison.png`
- Safari HEVC alpha decode: `/tmp/shogun-alpha-audit/hevc-alpha-1.3.png`
- Before/after alpha-edge comparison: `/tmp/shogun-alpha-compare.png`
- Hardened local opening state: `/tmp/shogun-alpha-local-opening.jpg`
- Hardened local open-product state: `/tmp/shogun-alpha-local-open2.jpg`
- Current review viewport: 1280 × 720; prior full-layout review viewport: 1440 × 900

## Intentional change

The first view keeps the existing Kyoto artwork, palette, copy, form, and proof badges unchanged. Only the product-film presentation changes: it begins as a closed MacBook, opens into the supplied live product demo, and then loops from the useful open-product state rather than closing again on every cycle.

The original film's dark rectangular backdrop is removed in browser-specific alpha-channel renditions: HEVC with alpha for Safari and VP9 with alpha for Chromium/Firefox. The alpha edge is hardened so low-opacity black spill from the source shadow cannot move across the page while the MacBook opens. A transparent closed-MacBook poster covers metadata loading, so the page background remains continuous before playback begins. The opaque MP4 fallback is intentionally removed; an unsupported browser keeps the transparent poster instead of revealing the moving black rectangle.

The Product Hunt proof badge now uses the official Featured widget supplied by the user, with its exact destination, image URL, 250 × 54 intrinsic dimensions, and product description.

## Visual review

- P0: none
- P1: none
- P2: none; the prior moving black edge halo is removed
- P3: none blocking handoff

The combined comparison confirms that the source film's laptop silhouette, bezel, opening motion, and product UI remain recognizable. In the implementation, the MacBook sits directly on the existing atmospheric hero without a dark video rectangle, card shell, ambient glow, or replacement background. The open product state stays the dominant right-column object and does not collide with the header, proof badges, or first logo row.

## Responsive, interaction, and state checks

- Desktop light, 1440 × 900: transparent MacBook media is fully visible and produces no horizontal overflow.
- Desktop dark, 1440 × 900: the same alpha media renders without introducing a separate background block.
- Initial loading: a 1200 × 904 transparent PNG shows the closed MacBook immediately.
- First playback: begins at 0 seconds so the physical opening motion is visible.
- Subsequent playback: loops from 4 seconds, avoiding a repetitive close/open reset.
- Reduced motion: seeks to the open-product preview at 4 seconds and pauses.
- Chromium compatibility: the browser selected the VP9 alpha WebM, reached ready state 4, and played the full opening sequence.
- Safari compatibility: AVFoundation decoded the HEVC `hvc1` MOV with alpha extrema 0–255 and transparent corner pixels.
- Hardened Safari alpha: at 1 second the visible silhouette begins at x=20 instead of leaking to the frame edge, and all pixels outside the MacBook remain transparent.
- Unsupported-video fallback: the transparent poster remains visible; there is no opaque video source.
- Asset loading: metadata preload avoids making the hero video a render-blocking resource.
- Product Hunt badge: the official external SVG loaded at its 250 × 54 intrinsic size, and the anchor uses the exact Featured campaign URL.
- Console: no new implementation error. Local development still reports the pre-existing missing PostHog token and React development-only CSP/eval warnings; neither is produced by the video or badge change.

## Iteration history

1. Replaced the synthetic hero panel with the supplied product film and enlarged it to the primary right-column visual.
2. Removed the surrounding frame, shadow, and ambient glow.
3. Shortened the left-side copy and preserved the conversion form and proof badges.
4. Reworked the film start state from an already-open demo to a closed MacBook that opens naturally.
5. Removed the baked-in dark backdrop with an alpha video and added a transparent closed-state poster.
6. Preserved the existing hero background in both themes and compared the source sequence and implementation in one visual input.
7. Added Safari-native HEVC alpha, removed the opaque MP4 fallback, and verified the complete opening motion in the in-app browser.
8. Replaced the Product Hunt review badge with the official Featured widget.
9. Removed the remaining moving black halo by tightening the HEVC/VP9 alpha silhouette and cache-busting all three hero media assets.

final result: passed
