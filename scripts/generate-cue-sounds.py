#!/usr/bin/env python3
"""Generate SHOGUN's six UI cue sounds (Issue #49, docs/sound-design.md §7).

The cues are a family, not six unrelated bleeps: every one is the same struck-bar timbre
(one fundamental plus the two inharmonic partials of an ideal free-free bar, 2.76x and 5.40x),
and they differ only in pitch motion and envelope. That is what makes them recognisable as
one product rather than a sound pack.

    ack-open   rising minor third      "something opened"
    ack-close  the same, inverted      "…and closed"  (audibly the pair of ack-open)
    ready      rising perfect fifth    "the thing you waited for is here"
    ask        one pitch, struck twice "you owe a decision" (no pitch motion: it must not rush you)
    fail       falling major second    "this needs you" — not an alarm; CLAUDE.md forbids
                                        interrupting the user's work
    signature  three notes, long tail  once in the life of an install

Output: 48 kHz / 16-bit / mono WAV, written to apps/desktop/src-tauri/sounds/.
Run:    python3 scripts/generate-cue-sounds.py        (stdlib only — no numpy)

Levels. Each file is normalised to a target loudness measured with a K-weighted mean square
(ITU-R BS.1770-4 pre-filter + RLB filter) over the whole cue. The gated integrated measure the
standard defines does not apply to sounds this short, so this is the honest approximation, and
the number printed below is that measure — not a certified LUFS reading. Sample peak is capped at -3 dBFS; true-peak still wants a real meter on device
(docs/sound-design.md §10).
"""

import math
import struct
import wave
from pathlib import Path

SAMPLE_RATE = 48_000
TARGET_LOUDNESS_DB = -26.0  # design doc §7.2
PEAK_CEILING_DB = -3.0
OUT_DIR = Path(__file__).resolve().parent.parent / "apps/desktop/src-tauri/sounds"

# Modes of an ideal bar with free ends: the "one source" every cue is built from.
PARTIALS = ((1.00, 1.00), (2.76, 0.34), (5.40, 0.11))  # (frequency ratio, amplitude)

# Equal-tempered pitches used below (Hz).
A5, C6, E6, G5 = 880.00, 1046.50, 1318.51, 783.99


def strike(freq: float, dur_s: float, decay_s: float, amp: float = 1.0) -> list[float]:
    """One struck note: instant-ish attack, exponential decay, higher partials dying sooner.

    The 4 ms raised-cosine attack is what keeps the onset from clicking (§7.2).
    """
    n = int(dur_s * SAMPLE_RATE)
    attack = int(0.004 * SAMPLE_RATE)
    out = [0.0] * n
    for i in range(n):
        t = i / SAMPLE_RATE
        env = math.exp(-t / decay_s)
        if i < attack:
            env *= 0.5 - 0.5 * math.cos(math.pi * i / attack)
        s = 0.0
        for ratio, level in PARTIALS:
            # Higher modes of a struck bar lose energy faster; without this the tail turns metallic.
            s += level * math.exp(-t * (ratio - 1.0) * 1.6 / decay_s) * math.sin(
                2.0 * math.pi * freq * ratio * t
            )
        out[i] = amp * env * s
    return out


def mix(length_s: float, *parts: tuple[float, list[float]]) -> list[float]:
    """Lay notes onto a fixed-length bed at (offset_seconds, samples)."""
    buf = [0.0] * int(length_s * SAMPLE_RATE)
    for offset_s, samples in parts:
        start = int(offset_s * SAMPLE_RATE)
        for i, v in enumerate(samples):
            j = start + i
            if 0 <= j < len(buf):
                buf[j] += v
    return buf


def fade_out(buf: list[float], ms: float = 45.0) -> list[float]:
    """End on true silence. A cue cut off mid-decay reads as a fault, not as a sound (§7.2)."""
    n = min(len(buf), int(ms / 1000.0 * SAMPLE_RATE))
    for i in range(n):
        buf[len(buf) - n + i] *= 1.0 - (i / n)
    buf[-1] = 0.0
    return buf


def biquad(buf: list[float], b: tuple[float, float, float], a: tuple[float, float]) -> list[float]:
    b0, b1, b2 = b
    a1, a2 = a
    x1 = x2 = y1 = y2 = 0.0
    out = [0.0] * len(buf)
    for i, x0 in enumerate(buf):
        y0 = b0 * x0 + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2
        out[i] = y0
        x2, x1 = x1, x0
        y2, y1 = y1, y0
    return out


def loudness_db(buf: list[float]) -> float:
    """K-weighted level over the whole cue, in a window of at least 400 ms.

    Short cues are zero-padded up to 400 ms so a 90 ms sound is not measured as if it played
    continuously; longer ones are measured end to end, tail included.
    """
    block = list(buf) + [0.0] * max(0, int(0.4 * SAMPLE_RATE) - len(buf))
    # BS.1770-4 stage 1 (head shelf) then stage 2 (RLB high-pass), coefficients at 48 kHz.
    k = biquad(block, (1.53512485958697, -2.69169618940638, 1.19839281085285),
               (-1.69065929318241, 0.73248077421585))
    k = biquad(k, (1.0, -2.0, 1.0), (-1.99004745483398, 0.99007225036621))
    mean_square = sum(v * v for v in k) / len(k)
    if mean_square <= 0.0:
        return -math.inf
    return -0.691 + 10.0 * math.log10(mean_square)


def normalise(buf: list[float]) -> tuple[list[float], float, float]:
    """Scale to the loudness target, then let the peak ceiling override it if they disagree."""
    measured = loudness_db(buf)
    gain = 10.0 ** ((TARGET_LOUDNESS_DB - measured) / 20.0)
    buf = [v * gain for v in buf]
    peak = max((abs(v) for v in buf), default=0.0)
    ceiling = 10.0 ** (PEAK_CEILING_DB / 20.0)
    if peak > ceiling:
        buf = [v * (ceiling / peak) for v in buf]
        peak = ceiling
    peak_db = 20.0 * math.log10(peak) if peak > 0 else -math.inf
    return buf, loudness_db(buf), peak_db


def write_wav(path: Path, buf: list[float]) -> None:
    frames = b"".join(
        struct.pack("<h", max(-32768, min(32767, int(round(v * 32767.0))))) for v in buf
    )
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SAMPLE_RATE)
        w.writeframes(frames)


def cues() -> dict[str, list[float]]:
    return {
        # Rising minor third — the opening gesture.
        "ack-open": mix(0.13, (0.0, strike(A5, 0.06, 0.030)), (0.045, strike(C6, 0.085, 0.038))),
        # The same interval falling, so start and end are heard as one pair.
        "ack-close": mix(0.15, (0.0, strike(C6, 0.06, 0.030)), (0.045, strike(A5, 0.105, 0.048))),
        # Rising fifth, second note left to ring: arrival, not motion.
        "ready": mix(0.26, (0.0, strike(A5, 0.075, 0.038)), (0.06, strike(E6, 0.20, 0.085))),
        # One pitch struck twice. Repetition asks; a rising interval would hurry the user.
        "ask": mix(0.30, (0.0, strike(C6, 0.10, 0.048)), (0.12, strike(C6, 0.18, 0.062))),
        # Falling major second, kept soft — this must be noticed, not obeyed.
        "fail": mix(0.22, (0.0, strike(A5, 0.07, 0.038)), (0.055, strike(G5, 0.16, 0.070))),
        # Three notes and a long tail. Heard once, when setup finishes.
        "signature": mix(
            0.62,
            (0.0, strike(A5, 0.16, 0.075)),
            (0.11, strike(C6, 0.16, 0.080)),
            (0.22, strike(E6, 0.40, 0.190)),
        ),
    }


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    rows = []
    for name, raw in cues().items():
        buf, loud, peak = normalise(fade_out(raw))
        path = OUT_DIR / f"{name}.wav"
        write_wav(path, buf)
        ms = len(buf) / SAMPLE_RATE * 1000.0
        rows.append((name, ms, loud, peak, path.stat().st_size))
        print(f"{name:<10} {ms:6.0f} ms  {loud:7.2f} dB (K-weighted)  "
              f"peak {peak:6.2f} dBFS  {path.stat().st_size / 1024:5.1f} KB")
    total = sum(r[4] for r in rows)
    print(f"{'total':<10} {'':>9} {'':>36} {'':>18} {total / 1024:5.1f} KB")


if __name__ == "__main__":
    main()
