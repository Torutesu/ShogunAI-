/**
 * Tiny UI sound engine — zero audio assets. Every sound is synthesized with
 * the Web Audio API (oscillators + gain envelopes), so the whole feature adds
 * no bytes to download and stays subtle by design.
 *
 * Rules baked in:
 *   - OFF by default; the visitor opts in via the 🔊 toggle (also the only way
 *     to satisfy the browser autoplay policy — the enabling click resumes the
 *     AudioContext).
 *   - Preference persists in localStorage ('shogun_sound').
 *   - Everything no-ops on the server and when disabled.
 */

const KEY = 'shogun_sound';

let ctx: AudioContext | null = null;
let master: GainNode | null = null;
let enabled = false;

function ensureCtx(): AudioContext | null {
  if (typeof window === 'undefined') return null;
  if (!ctx) {
    const AC = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!AC) return null;
    ctx = new AC();
    master = ctx.createGain();
    master.gain.value = 0.16; // keep the whole thing quiet/tasteful
    master.connect(ctx.destination);
  }
  if (ctx.state === 'suspended') void ctx.resume();
  return ctx;
}

/** One short tone with a fast attack + exponential decay. */
function tone(freq: number, dur: number, type: OscillatorType = 'sine', gain = 1, delay = 0): void {
  const c = ensureCtx();
  if (!c || !master) return;
  const osc = c.createOscillator();
  const g = c.createGain();
  osc.type = type;
  osc.frequency.value = freq;
  const t = c.currentTime + delay;
  g.gain.setValueAtTime(0.0001, t);
  g.gain.linearRampToValueAtTime(gain, t + 0.006);
  g.gain.exponentialRampToValueAtTime(0.0001, t + dur);
  osc.connect(g);
  g.connect(master);
  osc.start(t);
  osc.stop(t + dur + 0.03);
}

/** Read the persisted preference. Call once on mount. */
export function initSound(): boolean {
  try {
    enabled = localStorage.getItem(KEY) === 'on';
  } catch {
    enabled = false;
  }
  return enabled;
}

export function soundEnabled(): boolean {
  return enabled;
}

/** Toggle + persist. Enabling resumes the context and plays a confirm blip. */
export function setSound(on: boolean): void {
  enabled = on;
  try {
    localStorage.setItem(KEY, on ? 'on' : 'off');
  } catch {
    /* storage unavailable */
  }
  if (on) {
    ensureCtx();
    // gentle two-note confirm so the toggle is audibly "on"
    tone(660, 0.07, 'sine', 0.5);
    tone(990, 0.09, 'sine', 0.4, 0.06);
  }
}

export function playHover(): void {
  if (!enabled) return;
  tone(880, 0.045, 'sine', 0.22);
}

export function playClick(): void {
  if (!enabled) return;
  tone(300, 0.07, 'triangle', 0.45);
}

/** Rising three-note chime for a successful action (e.g. waitlist signup). */
export function playSuccess(): void {
  if (!enabled) return;
  tone(523.25, 0.12, 'sine', 0.55, 0); // C5
  tone(659.25, 0.12, 'sine', 0.55, 0.09); // E5
  tone(783.99, 0.2, 'sine', 0.55, 0.18); // G5
}
