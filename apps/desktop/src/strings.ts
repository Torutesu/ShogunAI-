// UI copy catalog — kept out of component markup per CLAUDE.md ("文言はコードから分離し
// i18n-readyに保つ"). v1 ships English only; the catalog shape is the i18n seam. Brand rules
// (CLAUDE.md): no competitor/stack names, the only emoji is ⚔, no "AI-powered/second brain".

export const STRINGS = {
  en: {
    // ── onboarding ────────────────────────────────────────────────────────────
    // First run, in the panel itself: the first thing you learn is where SHOGUN lives. The order
    // is deliberate — what it does, what it reads and never keeps, then permission, then the rest.
    // Nothing is asked for before the reason to grant it has been given.
    obNext: "Continue",
    obBack: "Back",
    obSkip: "Skip for now",
    obStep: "Step {n} of {total}",

    obWelcomeTitle: "SHOGUN lives in the notch.",
    obWelcomeBody:
      "It watches the work you're already doing, works out what you owe and what you're waiting on, and puts the next move one click away — right here, under the notch.",
    obWelcomePoint1: "Nothing to file, tag or maintain.",
    obWelcomePoint2: "It gets useful by tomorrow morning, and better every night.",
    obWelcomeStart: "Set up SHOGUN",

    obReadsTitle: "What it reads, and what it never keeps.",
    obReadsBody:
      "SHOGUN reads the text of the window you're working in, through the macOS Accessibility API. That's it.",
    obReadsKeep1Title: "No screenshots. Ever.",
    obReadsKeep1Body: "No images are captured, and none are stored. Text only.",
    obReadsKeep2Title: "Your work stays on this Mac.",
    obReadsKeep2Body:
      "Raw captures never leave the device. Only small processed chunks go out for indexing, and every one is logged where you can read it.",
    obReadsKeep3Title: "Nothing is sent on your behalf.",
    obReadsKeep3Body: "Replies, posts and invites wait for you to confirm them, one at a time.",
    obNeverTitle: "Never read at all",
    obNeverBody: "These are built in. There's nothing to configure, and they can't be turned off.",
    obNeverApps: { one: "1 app", other: "{n} apps" },
    obNeverRules: { one: "1 rule", other: "{n} rules" },
    /// Labels for the exclusion categories Rust reports — the ids come from the live policy, so a
    /// category added there shows up here as soon as it has a label.
    obExclusion: {
      password_managers: "Password managers",
      auth_dialog: "The macOS authentication dialog",
      terminals: "Terminals",
      private_browsing: "Private browsing windows",
      sensitive_titles: "Windows with sensitive titles",
    } as Record<string, string>,

    obPermTitle: "SHOGUN needs one permission.",
    obPermBody:
      "Accessibility lets it read the text of your active window. macOS will ask you to allow it in System Settings — SHOGUN can't grant it for you.",
    obPermGrant: "Open System Settings",
    obPermWaiting: "Waiting for permission…",
    obPermGranted: "Granted — it's reading",
    obPermProof: "Right now: {app}",
    obPermSkipTitle: "Without it",
    obPermSkipBody:
      "Connections and chat still work, but SHOGUN can't see what you're working on — so it can't tell you what's next, and drafts won't know your context.",

    obPlanTitle: "Seven days of everything.",
    obPlanBody:
      "Your trial includes every feature. When it ends, pick the plan you actually used — no card until then.",
    obPlanStandard: "Standard",
    obPlanStandardBody: "Capture, memory, search, the panel, read-only connections, nightly review.",
    obPlanPro: "Pro",
    obPlanProBody: "Everything in Standard, plus agents that act for you, the Memory API, and send.",
    obPlanKeys: "Two keys, two jobs",
    obPlanKeysBody:
      "Indexing and the nightly review run on SHOGUN's own key — included, nothing to set up. Agent work and chat run on your key, so your reasoning is yours and never metered by us.",
    obKeyTitle: "Your key",
    obKeyBody: "Stored in your macOS Keychain — never in a file, a database or a log.",

    obConnectTitle: "Connect what you work in.",
    obConnectBody:
      "First-layer connections talk to each service directly from this Mac. Read-only to begin with — you can add sending later, once you trust it.",
    obDraftStop: "Drafts only",
    obDraftStopBody: "Write replies and leave them in drafts. Nothing is ever sent for you.",
    obConnectSkip: "You can connect these later in Settings.",

    obReadyTitle: "You're set.",
    obReadyBody: "Two things worth knowing, and then it's out of your way.",
    obReadyShortcut: "Open SHOGUN from anywhere",
    obReadyDraft: "Draft where you're typing",
    obReadyDraftKey: "Tap ⌥",
    obReadyTonight: "Tonight",
    obReadyTonightBody:
      "While your Mac is idle, SHOGUN works through the day and updates what it's tracking. Tomorrow it'll know a little. By the weekend it'll know your week.",
    obReadyStart: "Start using SHOGUN",

    // ── panel ─────────────────────────────────────────────────────────────────
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
    /// Openers for an empty thread. These are prompts the user could have typed, offered because
    /// a blank panel is the worst moment to have to invent one — never invented facts, and the
    /// second only appears when something really is waiting. `{app}` is the app being read.
    suggestFirst: "What should I do first?",
    suggestWaiting: "What am I waiting on?",
    suggestCatchUp: "Catch me up on {app}",
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
    reconnect: "Sign in again",
    /// Second line of a service row before it is connected — what connecting it would buy you.
    /// Once connected the row shows its sync state instead, so these read as an offer, not a
    /// description of something already happening.
    connectionBlurbs: {
      gmail: "Read threads and draft replies. Sending always waits for you.",
      gcal: "Know what your day looks like before you ask.",
      gdrive: "Find the document you were about to look for.",
      slack: "Follow the threads you owe an answer to.",
      notion: "Keep your notes and pages in the picture.",
      github: "Track the reviews and issues waiting on you.",
      linear: "See the issues assigned to you in context.",
    } as Record<string, string>,
    // AI coding-tool transcripts (opt-in source)
    aiSessions: "AI sessions",
    aiSessionsHint:
      "Read your local AI coding-tool transcripts, so what you worked on and decided there is remembered. Stays on your device.",
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
    // errors
    sources: "Sources",
    noAnswer: "(no response)",
    answerFailed: "Couldn't answer",
  },
} as const;

export type Locale = keyof typeof STRINGS;

/** A count with its plural form. English needs one/other; the shape is what other locales need
 *  more of, so counted copy is written as a form table from the start rather than `+ " apps"`. */
export function count(forms: { one: string; other: string }, n: number): string {
  return (n === 1 ? forms.one : forms.other).replace("{n}", String(n));
}

/** Active locale (v1: English fixed; a settings-driven value in Phase 1). */
export const t = STRINGS.en;
