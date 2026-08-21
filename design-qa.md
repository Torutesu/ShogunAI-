# Design QA — Hero product video emphasis

## Evidence

- Source product asset: `/Users/torutano/Downloads/shogunheromac1200.mp4`
- Source video: 1200 × 904, 30 fps, 32.5 seconds, H.264, 1.1 MB
- Previous production hero capture: `/tmp/shogun-hero-final.png`
- Enlarged implementation capture: `/tmp/shogun-hero-larger.png`
- Side-by-side comparison: `/tmp/shogun-hero-size-comparison.png`
- State: Japanese locale, light theme, 1280 × 720

## Intentional change

The supplied live macOS product video is promoted to the visual focus of the first view. Its rendered desktop width increases from 542 px to 740 px while the left copy becomes shorter and quieter. The background treatment, conversion form, proof badges, and existing theme behavior remain intact.

## Visual review

- P0: none
- P1: none
- P2: none
- P3: none blocking handoff

The comparison confirms a clearer product-first hierarchy: the demo is materially larger, the Japanese explanation is reduced to three short lines, and the live registration count no longer adds noise beside the avatar proof. The laptop remains fully legible, the left conversion path stays clear, and all three proof badges remain visible in the 1280 × 720 first view.

## Responsive and state checks

- Desktop 1280 × 720: video renders at 740 × 557 px, plays muted and inline, and produces no horizontal overflow.
- Dark mode 1280 × 720: the larger source navy background and page atmosphere blend without a visible container boundary.
- Mobile 390 × 844: video is 350 px wide, follows the proof badges, and produces no horizontal overflow.
- Locales EN/JA/ES/DE at 1280 × 720: no horizontal overflow; locale-specific copy remains within the left column.
- Initial loading: 54 KB poster matches the first visible open-laptop state.
- Playback: starts at the useful product state, loops from that point, and pauses when reduced motion is requested.
- Asset loading: MP4 uses metadata preload rather than blocking the hero render.

## Iteration history

1. Replaced the synthetic hero panel with the supplied product video.
2. Removed frame, border, and card chrome around the media.
3. Added two-axis edge transparency and theme-aware ambient blending.
4. Added a matched poster, explicit dimensions, muted inline playback, and reduced-motion behavior.
5. Verified desktop light, desktop dark, and mobile layouts in the in-app browser.
6. Rebalanced the desktop columns, enlarged the video by about 36%, shortened all four localized hero descriptions, reduced headline scale, and removed the live count from the left proof row.

final result: passed
