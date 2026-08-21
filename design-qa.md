# Design QA — Hero MacBook opening sequence

## Evidence

- Source product film: `/Users/torutano/Downloads/shogunheromac1200.mp4`
- Source opening sequence: `/tmp/shogun-hero-frames/line-composite.jpg`
- Light-theme implementation: `/tmp/shogun-hero-final.png`
- Dark-theme implementation: `/tmp/shogun-hero-final-dark.png`
- Combined source/implementation review: `/tmp/shogun-hero-opening-comparison.png`
- Review viewport: 1440 × 900

## Intentional change

The first view keeps the existing Kyoto artwork, palette, copy, form, and proof badges unchanged. Only the product-film presentation changes: it begins as a closed MacBook, opens into the supplied live product demo, and then loops from the useful open-product state rather than closing again on every cycle.

The original film's dark rectangular backdrop is removed in a VP9 alpha-channel rendition. A transparent closed-MacBook poster covers metadata loading, so the page background remains continuous before playback begins. The original MP4 remains as a compatibility fallback.

## Visual review

- P0: none
- P1: none
- P2: none
- P3: none blocking handoff

The combined comparison confirms that the source film's laptop silhouette, bezel, opening motion, and product UI remain recognizable. In the implementation, the MacBook sits directly on the existing atmospheric hero without a dark video rectangle, card shell, ambient glow, or replacement background. The open product state stays the dominant right-column object and does not collide with the header, proof badges, or first logo row.

## Responsive, interaction, and state checks

- Desktop light, 1440 × 900: transparent MacBook media is fully visible and produces no horizontal overflow.
- Desktop dark, 1440 × 900: the same alpha media renders without introducing a separate background block.
- Initial loading: a 1200 × 904 transparent PNG shows the closed MacBook immediately.
- First playback: begins at 0 seconds so the physical opening motion is visible.
- Subsequent playback: loops from 4 seconds, avoiding a repetitive close/open reset.
- Reduced motion: seeks to the open-product preview at 4 seconds and pauses.
- Compatibility: WebM with alpha is preferred; the existing MP4 remains the fallback source.
- Asset loading: metadata preload avoids making the hero video a render-blocking resource.

## Iteration history

1. Replaced the synthetic hero panel with the supplied product film and enlarged it to the primary right-column visual.
2. Removed the surrounding frame, shadow, and ambient glow.
3. Shortened the left-side copy and preserved the conversion form and proof badges.
4. Reworked the film start state from an already-open demo to a closed MacBook that opens naturally.
5. Removed the baked-in dark backdrop with an alpha video and added a transparent closed-state poster.
6. Preserved the existing hero background in both themes and compared the source sequence and implementation in one visual input.

final result: passed
