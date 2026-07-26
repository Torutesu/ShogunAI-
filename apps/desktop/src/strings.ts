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
    welcomeSub: "Ask about your work, or tap ⌥ (Option) in any app to draft where you're typing.",
    noKey: "No key yet — add one in settings for real answers.",
    // composer
    ask: "Ask SHOGUN…",
    draftTitle: "Draft where you're typing (tap ⌥)",
    // settings
    settings: "Settings",
    // connections (first-layer integrations)
    connections: "Connections",
    connectionsHint: "First-layer integrations connect directly to each service. Data stays on your device.",
    connectionsEmpty: "Loading services…",
    connectionsUnavailable: "Not available yet",
    connect: "Connect",
    connecting: "Connecting…",
    disconnect: "Disconnect",
    // AI coding-tool transcripts (opt-in source)
    aiSessions: "AI sessions",
    aiSessionsHint:
      "Read your local AI coding-tool transcripts so SHOGUN remembers what you worked on and decided there. Stays on your device.",
    aiSessionsOn: "Importing",
    aiSessionsOff: "Off",
    // nightly cycle — what SHOGUN works out while you're away
    dream: "Nightly review",
    dreamHint: "While your Mac is idle overnight, SHOGUN works through the day and updates what it's tracking.",
    dreamNever: "Hasn't run yet.",
    dreamLocal: "Running on this device only.",
    dreamOk: "Last run",
    dreamCarried: "Last run didn't finish — it'll pick up tonight.",
    dreamAttention: "Hasn't finished for three nights.",
    dreamRunNow: "Run now",
    dreamRunning: "Running…",
    dreamEvents: "events",
    dreamChanges: "updates",
    dreamChunks: "sent",
    // approvals (L3 confirmation queue — anything leaving the device)
    approvals: "Approvals",
    approvalsHint: "Anything that leaves your device waits here for your explicit confirmation.",
    approvalsEmpty: "Nothing to confirm.",
    approvalsConfirm: "Confirm & send",
    approvalsReject: "Reject",
    approvalsVia: "third-party (Composio)",
    approvalsDirect: "direct",
    appearance: "Appearance",
    dark: "Dark",
    light: "Light",
    auto: "Auto",
    behavior: "When you look away",
    stayOpen: "Stay open",
    autoHide: "Auto-hide",
    stayOpenHint: "Keep the panel open until you close it.",
    autoHideHint: "Slide back to the notch when you move away.",
    draftShortcut: "Draft with SHOGUN",
    draftFixedHint: "Tap ⌥ (Option) alone — always on, not rebindable.",
    summonShortcut: "Show / hide overlay",
    quitShortcut: "Quit",
    shortcuts: "Shortcuts",
    change: "Change",
    recordHint: "Press keys… (Esc to cancel)",
    needModifier: "Include a modifier (⌃ ⌥ ⇧ ⌘).",
    shortcutHint: "Click a shortcut to change it. Saved instantly, works everywhere.",
    model: "Model",
    modelPlaceholder: "Model id (blank = default)",
    modelHint: "Chat and ⌥-tap drafts run on this provider with your own key. Each provider keeps its own key below.",
    key: "Your key",
    keyPresent: "Connected — answers and drafts are yours.",
    keyAbsent: "Not set — SHOGUN will echo until you add a key.",
    keyRejected: "This key was rejected. Check it, or pick another provider.",
    // Each provider keeps its own key; only the one selected above is ever used.
    keyScope: "Kept per provider — switching doesn't remove the others.",
    /// Per-provider placeholder — the key section follows the provider picked above it.
    keyPlaceholders: {
      anthropic: "Paste your Anthropic API key…",
      openrouter: "Paste your OpenRouter API key…",
      openai: "Paste your OpenAI API key…",
      gemini: "Paste your Gemini API key…",
    } as Record<string, string>,
    keySave: "Save",
    keyRemove: "Remove",
    keySaved: "Saved to your Keychain — answers are live.",
    quit: "Quit SHOGUN",
    quitTitle: "Quit SHOGUN",
    minimize: "Minimize to the notch",
    resizeHint: "Drag to resize",
    send: "Send",
    settingsTitle: "Settings",
    done: "Done",
    // state rows
    resolveHint: "Click to mark done",
    stateEmpty: "Nothing tracked yet.",
    // memory reset (deliberate, typed confirmation — context is foundational)
    memory: "Memory",
    memoryHint: "Removes the commitments and open loops SHOGUN extracted. Your captured history stays. This can't be undone.",
    memoryClear: "Clear extracted state",
    memoryConfirm: "Permanently delete {n} extracted items? Type CLEAR to confirm.",
    memoryConfirmPlaceholder: "Type CLEAR",
    memoryClearConfirm: "Delete",
    memoryCleared: "Cleared.",
    cancel: "Cancel",
    // meeting notes — the pill (FR-MT-08/09). "Notes", never "Recording": nothing is recorded,
    // and the word would promise a file that will never exist.
    meetingNotes: "Notes",
    meetingUntitled: "Meeting",
    meetingStarting: "Taking notes in",
    meetingStart: "Start",
    meetingNotNow: "Not now",
    meetingStop: "Stop",
    meetingNotePlaceholder: "Type your notes…",
    meetingNeverThisApp: "Never for this app",
    // meeting notes — settings (FR-MT-01/02/03)
    meetingSection: "Meeting notes",
    meetingHint: "Offers to take notes when a meeting starts. One tap declines, one tap stops.",
    meetingOn: "On",
    meetingOff: "Off",
    meetingExcluded: "Never offer for",
    meetingExcludedEmpty: "No apps excluded",
    meetingExcludedRemove: "Remove",
    // The disclosure of FR-MT-03. It can be stated this plainly because it is simply what
    // happens: nothing joins the call, and no audio file is ever written.
    meetingDisclosure:
      "Meetings are handled on this Mac. Nothing joins your call and no recording is kept — only the text stays.",
    // errors
    sources: "Sources",
    noAnswer: "(no response)",
    answerFailed: "Couldn't answer",
  },
} as const;

export type Locale = keyof typeof STRINGS;

/** Active locale (v1: English fixed; a settings-driven value in Phase 1). */
export const t = STRINGS.en;
