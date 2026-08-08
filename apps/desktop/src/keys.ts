// Shortcut rendering shared by Settings and the onboarding "ready" step. The bindings themselves
// live in Rust (app_data/shortcuts.json, `mod shortcuts`); this is only how a stored combo string
// is drawn as key chips. One copy, so the closing step of onboarding can never teach a different
// shortcut than Settings shows.

/** The defaults mirrored from Rust (`mod shortcuts` in lib.rs). Draft is not here: it fires on a
 *  bare ⌥ (Option) tap, which can't be a global shortcut and so isn't rebindable. */
export const DEFAULT_BINDS: Record<string, string> = {
  summon: "Control+Alt+KeyN",
  quit: "Control+Alt+KeyQ",
  voice: "Control+Alt+KeyV",
};

/** "Control+Alt+KeyN" → ["⌃","⌥","N"] for <kbd> chips. */
export function comboChips(combo: string): string[] {
  return combo.split("+").map((part) => {
    if (part === "Control") return "⌃";
    if (part === "Alt") return "⌥";
    if (part === "Shift") return "⇧";
    if (part === "Super") return "⌘";
    if (part.startsWith("Key")) return part.slice(3);
    if (part.startsWith("Digit")) return part.slice(5);
    return part;
  });
}
