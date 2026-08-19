# Shogun onboarding assets

Asset bundle is local-only. No runtime fetches or product-code changes are part of this task.

## Autumn gate image

- File: `gate-autumn-path.png`
- Intended use: meaningful gate artwork in onboarding's responsive right-side frame.
- Source: image supplied directly by user on 2026-08-19 as `ChatGPT Image Aug 19, 2026, 02_38_14 PM.png`.
- Original local source: `/Users/anantsinghal/Downloads/ChatGPT Image Aug 19, 2026, 02_38_14 PM.png`.
- Original and bundled SHA-256: `2ebfb14beb9fc89fc37e8ada4602c630a1756107e74b2b79292f46c5e2a7ee20`.
- Dimensions: 1,024 × 1,536 pixels; size: 3,437,886 bytes.
- Transformation: none; copied byte-for-byte.

## Autumn gate opening video

- File: `gate-opening.mp4`
- Intended use: completion-state onboarding gate animation.
- Source: generated from the user-supplied autumn gate image using OpenRouter Seedance 2.0 Fast on 2026-08-19.
- Format: MP4, 6 seconds, 3:4 portrait.
- SHA-256: `7fab1e0e139b73c716cc2be470bc2c0b04d78cef66ddb47ee98a1d25b8a960a3`.
- Transformation: none; original generated output copied unchanged.

## Yoiyami Core Theme

- File: `audio/yoiyami_core_theme.mp3`
- Intended use: quiet onboarding background music, played locally by native AVAudioPlayer.
- Author: Yoiyami
- License: CC0 1.0 Universal (public-domain dedication; no attribution required).
- Original listing: https://opengameart.org/content/yoiyami-core-theme-%E2%80%93-deep-blue-ambient-piano
- Original file URL: https://opengameart.org/sites/default/files/yoiyami_core_theme_0.wav
- Original downloaded file: `yoiyami_core_theme_0.wav`, 45,112,360 bytes (`Content-Length: 45112360`); `file` identified RIFF/WAVE Microsoft PCM, 16-bit, stereo, 48,000 Hz.
- Original SHA-256: `613d462f5229568ad98dcbe870036ccdf858f5ae33c63386cace86548809cb60`
- Distributable file: 5,639,661 bytes; SHA-256 `3d02d67888127350b447b3944759ea203694d80b139dfc12423caee6049efc24`.
- Transformation: `ffmpeg -hide_banner -y -i yoiyami_core_theme.wav -map_metadata -1 -codec:a libmp3lame -b:a 192k -write_xing 0 yoiyami_core_theme.mp3`.
- Derived metadata: MPEG Layer III, 192 kbps, 48 kHz, stereo, 234.984 seconds.
- CC0 evidence: OpenGameArt listing states `License(s): CC0`, identifies Yoiyami as author, and states commercial and non-commercial use; canonical deed: https://creativecommons.org/publicdomain/zero/1.0/.
- Original WAV is intentionally not bundled; only derived distributable MP3 remains.

## Lucide `waves`

- File: `icons/waves.svg`
- Intended use: decorative wave mark in onboarding.
- Author: Lucide contributors; portions held by Cole Bemis as Feather lineage.
- License: ISC License; full text in `licenses/ISC.txt`.
- Upstream tag: `v0.265.0`, pinned commit `9fb4b0b161fc256d2333f91812a927f2ed6f84c0`.
- Direct source: https://raw.githubusercontent.com/lucide-icons/lucide/v0.265.0/icons/waves.svg
- Pinned source view: https://github.com/lucide-icons/lucide/blob/9fb4b0b161fc256d2333f91812a927f2ed6f84c0/icons/waves.svg
- License source: https://raw.githubusercontent.com/lucide-icons/lucide/9fb4b0b161fc256d2333f91812a927f2ed6f84c0/LICENSE
- SHA-256: `de9e88a68ae884808e0852595e50cebd84b1a49b67ffecc8c7a888c629ab5b38`.
- Transformation: none. Upstream SVG geometry and attributes retained byte-for-byte; `stroke="currentColor"` allows host UI color inheritance.

## Fraunces variable font

- File: `fonts/Fraunces[SOFT,WONK,opsz,wght].woff2`
- Intended use: display heading typography in onboarding.
- Author: The Fraunces Project Authors / Undercase Type.
- License: SIL Open Font License 1.1; full text in `licenses/OFL-1.1.txt`.
- Upstream release: `1.000`, pinned commit `0bf87f6`.
- Official release archive: https://github.com/undercasetype/Fraunces/releases/download/1.000/UnderCaseType_Fraunces_1.000.zip
- Release archive SHA-256: `8d8b81dfaeb89433f5c908e1d8d0a4b202bd627bd80d4cd5ff56f311fdcad19f`.
- Extracted source path: `Fonts - Web/Fraunces[SOFT,WONK,opsz,wght].woff2`.
- Pinned source tree: https://github.com/undercasetype/Fraunces/tree/0bf87f6
- License source: https://raw.githubusercontent.com/undercasetype/Fraunces/0bf87f6/OFL.txt
- SHA-256: `25e420d8c154303e08322ea77f08997c4aade75653ef18425772ada5abacd0ce`.
- Size: 195,068 bytes; `file` identified WOFF2, TrueType flavor, version 1.0.
- Transformation: none; official release WOFF2 copied unchanged.

## Verification

- `file` checked audio, SVG, and WOFF2 containers.
- `ffprobe` checked original WAV and derived MP3 codecs, channels, sample rates, bitrate, duration, and size.
- `xmllint --noout icons/waves.svg` parsed SVG successfully.
- SVG inspection confirmed one 24×24 root and three upstream wave paths.
- Font metadata/container checked with `file`; the same official release's sibling variable TTF was inspected with `ttx` and reports family `Fraunces`, version `1.000`, and axes `opsz`, `wght`, `SOFT`, and `WONK`.
