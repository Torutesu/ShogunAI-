// Shortcut rendering shared by Settings and the onboarding "ready" step. The bindings themselves
// live in Rust (app_data/shortcuts.json, `mod shortcuts`); this is only how a stored combo string
// is drawn as key chips. One copy, so the closing step of onboarding can never teach a different
// shortcut than Settings shows.
//
// Combo grammar (mirrored from Rust):
//   "Control+Alt+KeyN"  — plugin-registered chord (modifiers + a real key)
//   "Tap+Alt"           — a solo tap of one modifier (draft's default ⌥ tap)
//   "Dual+Super"        — left and right of the same modifier pressed together (recall's ⌘⌘)

/** The defaults mirrored from Rust (`mod shortcuts` in lib.rs). */
export const DEFAULT_BINDS: Record<string, string> = {
  draft: "Tap+Alt",
  recall: "Dual+Super",
  summon: "Control+Alt+KeyN",
  quit: "Control+Alt+KeyQ",
  voice: "Control+Alt+KeyV",
};

const MOD_GLYPH: Record<string, string> = {
  Control: "⌃",
  Alt: "⌥",
  Shift: "⇧",
  Super: "⌘",
  Fn: "Fn",
};

/** "Control+Alt+KeyN" → ["⌃","⌥","N"]; "Tap+Alt" → ["⌥ tap"]; "Dual+Super" → ["⌘","⌘"]. */
export function comboChips(combo: string): string[] {
  if (combo.startsWith("Tap+")) {
    const g = MOD_GLYPH[combo.slice(4)] ?? combo.slice(4);
    return [`${g} tap`];
  }
  if (combo.startsWith("Dual+")) {
    const g = MOD_GLYPH[combo.slice(5)] ?? combo.slice(5);
    return [g, g];
  }
  return combo.split("+").map((part) => {
    if (part in MOD_GLYPH) return MOD_GLYPH[part];
    if (part.startsWith("Key")) return part.slice(3);
    if (part.startsWith("Digit")) return part.slice(5);
    return part;
  });
}
