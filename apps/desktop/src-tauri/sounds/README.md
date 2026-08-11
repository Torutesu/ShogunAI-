# Cue sounds

The six UI cues (Issue #49, `docs/sound-design.md` §7). Bundled as a resource and preloaded at
startup by `src/sound.rs`.

## Provenance and licence

**Generated, not sampled.** Every file here is synthesised by
[`scripts/generate-cue-sounds.py`](../../../../scripts/generate-cue-sounds.py) from pure maths —
no recordings, no sample libraries, no third-party assets, so there is no upstream licence to
honour. They are part of this repository and carry its licence.

To change a cue, edit the script and re-run it; do not hand-edit the WAVs.

```
python3 scripts/generate-cue-sounds.py
```

## The files

| File | Cues that use it | Gesture | Length |
|---|---|---|---|
| `ack-open.wav` | Summon, voice start | rising minor third | 130 ms |
| `ack-close.wav` | Voice end | the same, inverted | 150 ms |
| `ready.wav` | Recap ready, model ready, connector linked | rising perfect fifth | 260 ms |
| `ask.wav` | Approval pending, meeting offered | one pitch struck twice | 300 ms |
| `fail.wav` | Voice failed, capture stopped | falling major second | 220 ms |
| `signature.wav` | Onboarding complete, launch (opt-in) | three notes, long tail | 620 ms |

48 kHz / 16-bit / mono WAV, 158 KB in total. Uncompressed on purpose: decoding must not sit
between a cue and the SLO it shares a frame with.

Levels as generated: −26 dB K-weighted (see the script's docstring for exactly what is measured),
sample peak between −14 and −10 dBFS, no clipping, every file starting and ending at true silence.
True-peak and on-device level still want a check on real hardware (`docs/sound-design.md` §10).

## Not audio data

These are program assets. Invariant 2 ("no audio is ever stored") is about **the user's** audio —
microphone, meeting and system capture, none of which is ever written to disk. Nothing here is
recorded from anyone, and nothing here is ever written back.
