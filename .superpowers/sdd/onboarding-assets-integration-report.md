# Onboarding asset integration report

Date: 2026-08-19
Worktree: `/private/tmp/shogun-onboarding`
Branch: `codex-mikel/onboarding`
Scope: licensed local assets only. No React, CSS, Rust, package manifest, lockfile, or product behavior changes.

## Added files

- `apps/desktop/src/assets/onboarding/audio/yoiyami_core_theme.mp3` — 5,639,661 bytes, 192 kbps MP3.
- `apps/desktop/src/assets/onboarding/icons/waves.svg` — 517 bytes, Lucide `waves`.
- `apps/desktop/src/assets/onboarding/fonts/Fraunces[SOFT,WONK,opsz,wght].woff2` — 195,068 bytes, Fraunces variable WOFF2.
- `apps/desktop/src/assets/onboarding/licenses/ISC.txt` — exact Lucide ISC notice.
- `apps/desktop/src/assets/onboarding/licenses/OFL-1.1.txt` — exact Fraunces OFL 1.1 text.
- `apps/desktop/src/assets/onboarding/manifest.md` — sources, authors, licenses, pins, hashes, transformations, intended use, and verification notes.

## Sources and provenance

### Music

- Official listing: https://opengameart.org/content/yoiyami-core-theme-%E2%80%93-deep-blue-ambient-piano
- Original direct file: https://opengameart.org/sites/default/files/yoiyami_core_theme_0.wav
- Author: Yoiyami.
- License: CC0 1.0 Universal. Listing identifies `License(s): CC0` and permits commercial/non-commercial use.
- Original source file: `yoiyami_core_theme_0.wav`, 45,198,376 bytes.
- Original SHA-256: `cabef49063f4218c8e005b8958f4e4351de93619375b544c92f17b9cf50c0aa1`.
- Derived MP3 SHA-256: `8fde4701bb432e51380bd1c50f8f860bcd1e29e30a2554db6ff589ae9edba0ba`.
- Conversion command:

  ```sh
  ffmpeg -hide_banner -y -i apps/desktop/src/assets/onboarding/audio/yoiyami_core_theme.wav -map_metadata -1 -codec:a libmp3lame -b:a 192k -write_xing 0 apps/desktop/src/assets/onboarding/audio/yoiyami_core_theme.mp3
  ```

  Source WAV was verified, hashed, converted, then removed. Only distributable derived MP3 remains.

### Icon

- Direct source: https://raw.githubusercontent.com/lucide-icons/lucide/v0.265.0/icons/waves.svg
- Pinned tag: `v0.265.0`.
- Pinned commit: `9fb4b0b161fc256d2333f91812a927f2ed6f84c0`.
- Author/license: Lucide contributors, ISC License; full exact text bundled in `licenses/ISC.txt`.
- SHA-256: `de9e88a68ae884808e0852595e50cebd84b1a49b67ffecc8c7a888c629ab5b38`.
- Transformation: none; upstream geometry/attributes retained, including `stroke="currentColor"`.

### Typeface

- Official release archive: https://github.com/undercasetype/Fraunces/releases/download/1.000/UnderCaseType_Fraunces_1.000.zip
- Release: `1.000`; pinned commit: `0bf87f6`.
- Release archive SHA-256: `8d8b81dfaeb89433f5c908e1d8d0a4b202bd627bd80d4cd5ff56f311fdcad19f`.
- Extracted path: `Fonts - Web/Fraunces[SOFT,WONK,opsz,wght].woff2`.
- Author/license: The Fraunces Project Authors / Undercase Type, SIL Open Font License 1.1; full exact text bundled in `licenses/OFL-1.1.txt`.
- WOFF2 SHA-256: `25e420d8c154303e08322ea77f08997c4aade75653ef18425772ada5abacd0ce`.
- Transformation: none; copied from official release archive.

## Exact validation commands and results

```sh
file apps/desktop/src/assets/onboarding/audio/yoiyami_core_theme.wav
# RIFF (little-endian) data, WAVE audio, Microsoft PCM, 16 bit, stereo 48000 Hz

ffprobe -v error -show_entries format=format_name,duration,size:stream=codec_name,sample_rate,channels,bits_per_sample -of default=noprint_wrappers=1 apps/desktop/src/assets/onboarding/audio/yoiyami_core_theme.wav
# codec_name=pcm_s16le; sample_rate=48000; channels=2; bits_per_sample=16; format_name=wav

file apps/desktop/src/assets/onboarding/audio/yoiyami_core_theme.mp3
# MPEG ADTS, layer III, v1, 192 kbps, 48 kHz, JntStereo

ffprobe -v error -show_entries format=format_name,duration,size:stream=codec_name,bit_rate,sample_rate,channels -of default=noprint_wrappers=1 apps/desktop/src/assets/onboarding/audio/yoiyami_core_theme.mp3
# codec_name=mp3; sample_rate=48000; channels=2; bit_rate=192000; format_name=mp3; duration=234.984000

file apps/desktop/src/assets/onboarding/icons/waves.svg
# SVG Scalable Vector Graphics image
xmllint --noout apps/desktop/src/assets/onboarding/icons/waves.svg
# success

file 'apps/desktop/src/assets/onboarding/fonts/Fraunces[SOFT,WONK,opsz,wght].woff2'
# Web Open Font Format (Version 2), TrueType, length 195068, version 1.0

ttx -l '.asset-work/fraunces/UnderCaseType_Fraunces_1.000/Fonts - Desktop/Fraunces[SOFT,WONK,opsz,wght].ttf'
# official sibling variable font contains fvar, name, STAT, avar, and gvar tables
ttx -t name -t fvar -o .asset-work/font-meta.ttx '.asset-work/fraunces/UnderCaseType_Fraunces_1.000/Fonts - Desktop/Fraunces[SOFT,WONK,opsz,wght].ttf'
# family Fraunces, Version 1.000, axes opsz/wght/SOFT/WONK

git diff --check
# success
```

`fc-scan` could not emit metadata in this sandbox because its default cache directories are not writable and this build lacks a WOFF2 decoder. Container validation used `file`; metadata validation used the official release's sibling variable TTF with `ttx`, which confirms the same family/version/axis payload represented by the WOFF2 release file.

## Commit

`6c30ac3` (`chore(desktop): add licensed onboarding assets`). Report content is amended in this commit to record final hash.

## Concerns

- OpenGameArt page and CC0 declaration are preserved as provenance, but no separate Content ID/fingerprint clearance was performed.
- Audio playback wiring is intentionally absent; this task adds assets only.
