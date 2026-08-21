# Design QA — Hero product video

## Evidence

- Source product asset: `/Users/torutano/Downloads/shogunheromac1200.mp4`
- Source video: 1200 × 904, 30 fps, 32.5 seconds, H.264, 1.1 MB
- Previous production hero capture: `/tmp/shogun-hero-before.png`
- Implementation capture: `/tmp/shogun-hero-final.png`
- Side-by-side comparison: `/tmp/shogun-hero-qa-comparison.png`
- State: Japanese locale, light theme, 1280 × 720

## Intentional change

The right-side static product control panel is replaced with the supplied live macOS product video. The video has no card chrome; its dark source background is feathered to transparency on every edge and supported by a restrained ambient glow so it blends with both the Kyoto artwork and the dark hero theme.

## Visual review

- P0: none
- P1: none
- P2: none
- P3: none blocking handoff

The video remains the same visual weight as the previous mockup, preserves the existing two-column hero hierarchy, and does not compete with the headline or waitlist CTA. The laptop silhouette remains legible while the source rectangle no longer ends in a hard edge.

## Responsive and state checks

- Desktop 1280 × 720: video plays muted and inline; no horizontal overflow.
- Dark mode 1280 × 720: the source navy background and page atmosphere blend without a visible container boundary.
- Mobile 390 × 844: video is 350 px wide, follows the proof badges, and produces no horizontal overflow.
- Initial loading: 54 KB poster matches the first visible open-laptop state.
- Playback: starts at the useful product state, loops from that point, and pauses when reduced motion is requested.
- Asset loading: MP4 uses metadata preload rather than blocking the hero render.

## Iteration history

1. Replaced the synthetic hero panel with the supplied product video.
2. Removed frame, border, and card chrome around the media.
3. Added two-axis edge transparency and theme-aware ambient blending.
4. Added a matched poster, explicit dimensions, muted inline playback, and reduced-motion behavior.
5. Verified desktop light, desktop dark, and mobile layouts in the in-app browser.

final result: passed
