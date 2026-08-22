# Design QA — Hero MacBook product demo

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
- Same-viewport black-transition before/open-product after comparison: `/tmp/shogun-hero-black-before-after.jpg`
- Latest local open-product state: `/tmp/shogun-hero-after-open.jpg`
- Restored closed-PC state at 0.76 seconds: `/tmp/shogun-hero-opening-final-0.76.jpg`
- Screen reveal at 1.91 seconds: `/tmp/shogun-hero-opening-final-1.91.jpg`
- Fully opaque product state at 3.02 seconds: `/tmp/shogun-hero-opening-final-3.02.jpg`
- Live product state at 4.95 seconds: `/tmp/shogun-hero-opening-final-4.95.jpg`
- Same-viewport moving-black before/current-opening after comparison: `/tmp/shogun-hero-opening-before-after-final-v4.jpg`
- Current review viewport: 1280 × 720; prior full-layout review viewport: 1440 × 900

## Intentional change

The first view keeps the existing Kyoto artwork, palette, copy, form, and proof badges unchanged. The supplied product film now starts with the closed MacBook and plays its opening once. Only the moving dark lid plane is suppressed during that opening, so the Kyoto background remains visually fixed rather than moving with a black rectangle.

The browser-specific alpha renditions remain HEVC with alpha for Safari and VP9 with alpha for Chromium/Firefox. The moving lid region stays fully transparent while the closed laptop base remains visible, then the complete screen fades in from 2.05 to 2.3 seconds without punching holes into dark UI. A transparent closed-state poster covers metadata loading. After the first complete playthrough, subsequent loops resume at four seconds so the opening does not repeat continuously. The opaque MP4 fallback remains intentionally absent.

The Product Hunt proof badge now uses the official Featured widget supplied by the user, with its exact destination, image URL, 250 × 54 intrinsic dimensions, and product description.

## Visual review

- P0: none
- P1: none
- P2: none; the prior moving black lid/background plane is removed without deleting the closed-PC opening
- P3: none blocking handoff

The same-viewport comparison confirms that the original black slab no longer travels across the Kyoto scene. The closed MacBook base remains present at the start, then the fully opaque screen and product UI appear in the same right-column footprint. The media introduces no card shell, ambient glow, replacement background, header collision, proof-badge collision, or horizontal overflow.

## Responsive, interaction, and state checks

- Desktop light, 1440 × 900: transparent MacBook media is fully visible and produces no horizontal overflow.
- Desktop dark, 1440 × 900: the same alpha media renders without introducing a separate background block.
- Initial loading: a transparent PNG shows the closed MacBook without a black rectangle.
- First playback: preserves the closed-PC start and opening sequence while keying the moving dark lid plane transparent.
- Subsequent playback: resumes at four seconds and does not replay the opening on every loop.
- Reduced motion: seeks to four seconds, displays the fully open product state, and pauses.
- Chromium compatibility: the browser selected the restored VP9 alpha WebM, reached ready state 4, and played continuously; exact browser samples at 0.76, 1.91, 3.02, and 4.95 seconds confirm the fixed base, short screen reveal, and fully opaque product state.
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
10. Temporarily removed the dark opening segment and used the open product as the loading poster.
11. Restored the closed-PC opening, kept the moving dark lid plane transparent, faded the complete screen in from 2.05 to 2.3 seconds, and made later loops resume at four seconds.
12. Restored the localized live waitlist count beside the participant avatars, with a stable server fallback and an API refresh on page load and after successful signup.
13. Removed the late-film dark flash and loop jump by ending the one-time demo on a stable open-product frame before the source recording fades to black.

## Live participant count QA

- Public before: `/tmp/shogun-count-before.jpg`
- Local after: `/tmp/shogun-count-after.jpg`
- Same-viewport comparison: `/tmp/shogun-count-before-after.jpg`
- Review viewport: 1280 × 720
- The count pill replaces the non-numeric "buzzing" label without changing the avatar stack, form width, badge row, or hero grid.
- The localized number and suffix fit in Japanese at the reviewed desktop width without wrapping or overlapping adjacent content.
- The component renders the server fallback immediately to avoid layout shift, then refreshes from `/api/waitlist/count` with `cache: 'no-store'`.
- A successful waitlist submission dispatches a local refresh event so the displayed total is reconciled with the durable public count.
- The public count endpoint returned `523` with `fresh: true` during this review.

## Stable hero demo ending QA

- Source recording review: `/tmp/shogun-black-jitter-17-25.jpg`
- Hero-video timeline review: `/tmp/shogun-hero-21-26-timeline.jpg`
- Same-viewport recording/local comparison: `/tmp/shogun-jitter-before-after-final.png`
- Stable local frame, first sample: `/tmp/shogun-jitter-local-final-held-2.png`
- The supplied demo still begins with the closed MacBook and plays the complete opening sequence once.
- Playback now pauses at 24.4 seconds on the stable open-product dashboard, before the source film's broad dark fade begins.
- The final frame remains fixed, so the Kyoto background does not appear to flash, jump, or restart beneath the MacBook.
- Browser state remained exactly `currentTime: 24.4`, `paused: true`, and `readyState: 4` across a further four-second observation.
- Reduced-motion behavior remains unchanged at the stable four-second open-product preview.
- A frame-callback watch provides frame-level stopping in supported browsers; `timeupdate` remains as the compatibility fallback.

final result: passed

## Mobile hero centering and Japanese subtitle QA

- Source visual truth: `/var/folders/73/8h5shzqn3nj4zmn32ntdtp6c0000gn/T/TemporaryItems/NSIRD_screencaptureui_It8eZ1/スクリーンショット 2026-08-22 14.41.43.png`
- Implementation screenshot: `/tmp/shogun-hero-centered-430.png`
- Combined comparison: `/tmp/shogun-hero-centering-comparison.png`
- Viewport: `430 × 932` CSS px, light theme, Japanese locale, initial hero state
- Source pixels: `1086 × 1572`; the `856 × 1572` page-content crop was normalized to `430 × 790`
- Implementation pixels: `430 × 932` at device scale factor `1`

The normalized source and implementation were compared side by side. The headline bounding box is `x=61.33`, `width=307.34` inside a `430px` viewport, giving a center of `215px`, exactly matching the viewport center.

- P0: none
- P1: none
- P2: none after centering the headline on mobile and retaining desktop left alignment
- Typography: existing family, size, weight, line height, tracking, and two-line wrap are preserved.
- Spacing and layout rhythm: only the headline's horizontal placement changed.
- Colors and visual tokens: unchanged.
- Image quality and asset fidelity: unchanged.
- Copy: `人に届くものだけ、あなたの承認を待ちます。` was removed from the Japanese hero subtitle.

Iteration: the earlier mobile headline max-width box aligned to its parent's left edge. Adding mobile auto inline margins and resetting them at the desktop breakpoint moved the rendered center to `215px` without surrounding layout drift.

final result: passed
