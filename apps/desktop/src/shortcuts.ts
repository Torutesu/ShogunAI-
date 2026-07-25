// The trigger grammar.
//
// A shortcut here is usually NOT a letter chord. It is a set of modifiers plus a gesture — hold
// them, tap them, tap them twice — because that is what a permanently-resident overlay needs:
//
// - Nothing to memorise. "Hold control-option" is a thing your hands learn in a day; ⌃⌥N is a
//   thing you look up.
// - Nothing to collide with. Every letter chord is somebody's shortcut in some app. A modifier
//   set with no key is nobody's.
// - It already existed here, unexplained. The ⌥-tap that drafts at your cursor was described in
//   the code as a special case that "can't be a global shortcut and so isn't rebindable". It is
//   not a special case; it is `Alt:tap`, one sentence in this grammar, and once the grammar is
//   written down it is as rebindable as anything else.
//
// Letter chords remain expressible, and remain the right answer for rare or destructive actions
// (Quit): reaching for ⌃⌥Q is *supposed* to take a deliberate moment.
//
// Serialisation is a string, so the existing `get_shortcuts` / `set_shortcut` commands carry it
// unchanged: "Control+Alt:double", "Alt:tap", "Function+Control:hold", and the legacy key form
// "Control+Alt+KeyN" (which parses as gesture `press`).

export type Mod = "Function" | "Control" | "Alt" | "Shift" | "Super";

/** What your hands do with the modifiers. `press` is the legacy key-chord form. */
export type Gesture = "hold" | "tap" | "double" | "press";

export interface Trigger {
  mods: Mod[];
  gesture: Gesture;
  /** Only for `press` — the physical key code, e.g. "KeyN". */
  key?: string;
}

/** Canonical modifier order, so two equal triggers always serialise identically. */
const ORDER: Mod[] = ["Function", "Control", "Alt", "Shift", "Super"];

const MOD_SET = new Set<string>(ORDER);

export function parseTrigger(s: string): Trigger | null {
  if (!s) return null;
  const [chord, gesture] = s.split(":");
  const parts = chord.split("+").filter(Boolean);
  const mods = parts.filter((p): p is Mod => MOD_SET.has(p));
  const key = parts.find((p) => !MOD_SET.has(p));
  if (mods.length === 0) return null;
  mods.sort((a, b) => ORDER.indexOf(a) - ORDER.indexOf(b));
  if (key) return { mods, gesture: "press", key };
  const g = gesture as Gesture;
  return { mods, gesture: g === "hold" || g === "tap" || g === "double" ? g : "hold" };
}

export function formatTrigger(t: Trigger): string {
  const mods = [...t.mods].sort((a, b) => ORDER.indexOf(a) - ORDER.indexOf(b));
  if (t.gesture === "press" && t.key) return [...mods, t.key].join("+");
  return `${mods.join("+")}:${t.gesture}`;
}

/** Two triggers collide when the same hands do the same thing. */
export function sameTrigger(a: Trigger, b: Trigger): boolean {
  return formatTrigger(a) === formatTrigger(b);
}

/**
 * Whether `a` swallows `b` — fires first and leaves nothing for it.
 *
 * A tap of ⌃⌥ also happens on the way into a HOLD of ⌃⌥ and on the way into a DOUBLE of ⌃⌥, so a
 * tap shadows both. It does NOT shadow a key chord on the same modifiers: a real key joining
 * cancels the pending tap, which is what lets `Alt:tap` and `Control+Alt+KeyQ` coexist — the
 * recogniser must behave this way, so the warning must not claim otherwise.
 *
 * Reported as a warning, never a refusal: it is the user's keyboard.
 */
export function shadows(a: Trigger, b: Trigger): boolean {
  if (sameTrigger(a, b)) return true;
  const sameMods = a.mods.length === b.mods.length && a.mods.every((m) => b.mods.includes(m));
  if (!sameMods) return false;
  return a.gesture === "tap" && (b.gesture === "hold" || b.gesture === "double");
}

/** Every action the panel can be reached by. Order is the order they appear in Settings. */
export const ACTIONS = ["summon", "draft", "quit"] as const;
export type Action = (typeof ACTIONS)[number];

/**
 * Defaults, and the argument for each:
 * - `summon` — the thing you do twenty times a day, so it costs no key at all. Double-tap rather
 *   than hold, because holding ⌃⌥ is something you do by accident while reaching for a chord.
 * - `draft`  — a single ⌥ tap, unchanged. It is the fastest gesture there is, and it is spent on
 *   the action that has to happen without leaving the sentence you are typing.
 * - `quit`   — a letter chord, deliberately. Quitting a resident app should take a moment's aim.
 */
export const DEFAULT_TRIGGERS: Record<Action, string> = {
  summon: "Control+Alt:double",
  draft: "Alt:tap",
  quit: "Control+Alt+KeyQ",
};

/** Display parts for a modifier: the glyph your keyboard shows, and the word printed on it. */
export const MOD_LABEL: Record<Mod, { glyph: string; name: string }> = {
  Function: { glyph: "", name: "fn" },
  Control: { glyph: "⌃", name: "control" },
  Alt: { glyph: "⌥", name: "option" },
  Shift: { glyph: "⇧", name: "shift" },
  Super: { glyph: "⌘", name: "command" },
};

/** "KeyN" → "N", "Digit2" → "2", anything else as-is. */
export function keyLabel(code: string): string {
  if (code.startsWith("Key")) return code.slice(3);
  if (code.startsWith("Digit")) return code.slice(5);
  return code;
}
