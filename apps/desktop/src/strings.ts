// UI copy catalog — kept out of component markup per CLAUDE.md ("文言はコードから分離し
// i18n-readyに保つ"). v1 ships English only; the catalog shape is the i18n seam. Brand rules
// (CLAUDE.md): no competitor/stack names, the only emoji is ⚔, no "AI-powered/second brain".

export const STRINGS = {
  en: {
    // live line
    reading: "reading",
    yourScreen: "your screen",
    // counts chip
    due: "due",
    waiting: "waiting",
    // peek (hover preview)
    peekHint: "click to open",
    openPanel: "Open SHOGUN (⌃⌥N)",
    // welcome (expanded, empty thread)
    welcomeTitle: "What can I take off your plate?",
    welcomeSub: "Ask about your work, or press ⌃⌥G in any app to draft where your cursor is.",
    noKey: "No key yet — add one in settings for real answers.",
    // composer
    ask: "Ask SHOGUN…",
    draftTitle: "Draft where your cursor is (⌃⌥G)",
    // settings
    settings: "Settings",
    appearance: "Appearance",
    dark: "Dark",
    light: "Light",
    auto: "Auto",
    behavior: "When you look away",
    stayOpen: "Stay open",
    autoHide: "Auto-hide",
    stayOpenHint: "Keep the panel open until you close it.",
    autoHideHint: "Slide back to the notch when you move away.",
    draftShortcut: "Draft at cursor",
    summonShortcut: "Bring to this desktop",
    quitShortcut: "Quit",
    shortcuts: "Shortcuts",
    key: "Your key",
    keyPresent: "Connected — answers and drafts are yours.",
    keyAbsent: "Not set — SHOGUN will echo until you add a key.",
    quit: "Quit SHOGUN",
    done: "Done",
    // errors
    noAnswer: "(no response)",
    answerFailed: "Couldn't answer",
  },
} as const;

export type Locale = keyof typeof STRINGS;

/** Active locale (v1: English fixed; a settings-driven value in Phase 1). */
export const t = STRINGS.en;
