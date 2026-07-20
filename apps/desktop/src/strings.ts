// UI copy catalog — kept out of component markup per CLAUDE.md ("文言はコードから分離し
// i18n-readyに保つ"). v1 ships English only; the catalog shape is the i18n seam.

export const STRINGS = {
  en: {
    action1: "Action 1",
    action2: "Action 2",
    action3: "Action 3",
    action4: "Action 4",
    openFullUi: "Open Full UI",
    partialSuffix: " (partial)",
    noContext: "no context",
    noText: "no readable text",
    charsCaptured: "chars",
  },
} as const;

export type Locale = keyof typeof STRINGS;

/** Active locale (v1: English fixed; a settings-driven value in Phase 1). */
export const t = STRINGS.en;
