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
    // The separate window (spec §D) — where the brief, health, memory and logs live.
    openFullUi: "Open SHOGUN window",
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
    // errors
    sources: "Sources",
    noAnswer: "(no response)",
    answerFailed: "Couldn't answer",
  },
} as const;

/** Full UI (the separate window, spec §D). Kept in its own catalog so the dense notch panel and
 *  the roomier window can diverge in wording without stepping on each other. English only in v1;
 *  same i18n seam as STRINGS above. */
export const FULL_UI = {
  en: {
    // navigation
    navToday: "Today",
    navHealth: "Context Health",
    navSources: "Sources",
    navMemory: "Memory",
    navActivity: "Activity",
    navTrace: "Traceability",
    groupContext: "Where it comes from",
    groupDid: "What it did",
    planTrial: "Trial",
    planStandard: "Standard",
    // pane subtitles
    todaySub: "Your brief, your schedule, and what to do about it.",
    healthSub: "What SHOGUN can and can't see right now — every number has a way to fix it.",
    sourcesSub: "Where context comes from, how fresh it is, and what's excluded.",
    memorySub: "Everything SHOGUN has extracted, with the evidence behind it.",
    activitySub: "What ran, at what level, and whether anything left.",
    traceSub: "Every byte that left this device — digest and size only, never content.",
    // health
    confidenceMix: "Confidence mix",
    slo: "SLO",
    high: "High",
    medium: "Medium",
    low: "Low",
    notOnThisPlan: "— not on this plan",
    // today
    morningBrief: "Morning brief",
    notMeasuredYet: "Not measured yet.",
    briefNeverRun: "No brief yet — the first one is assembled after a nightly review runs.",
    briefDegraded: "The nightly review didn't finish, so this is your calendar and overdue commitments only.",
    suggested: "Suggested actions",
    lockedNeedsKey: "Drafting runs on your own key — that comes with Pro.",
    schedule: "Schedule",
    prep: "Prep",
    // sources
    connectedServices: "Connected services",
    sourcesHint: "Each one connects directly. Scope is what SHOGUN is allowed to read.",
    exclusions: "Capture exclusions",
    exclusionsHint: "Apps and windows SHOGUN never reads. Excluded time still counts against coverage.",
    alwaysExcluded: "Always excluded",
    healthy: "Healthy",
    needsAttention: "Needs attention",
    thirdParty: "third-party",
    direct: "Direct",
    on: "On",
    off: "Off",
    // memory
    commitments: "Commitments",
    commitmentsHint: "Every row carries the evidence it came from and how sure SHOGUN is.",
    possibly: "possibly:",
    why: "Why?",
    needsYourEye: "Needs your eye",
    mergeHint: "These look like the same person, but not confidently enough to merge on their own.",
    keepSeparate: "Keep separate",
    merge: "Merge",
    // activity
    waitingForYou: "Waiting for you",
    review: "Review",
    runHistory: "Run history",
    noRunsExplained:
      "Nothing has run. Agents are what carry an action out, and they come with Pro — so there's no history here rather than an empty one.",
    lastNightly: "Last nightly cycle",
    finishedAt: "Finished",
    eventsRead: "events read",
    updates: "updates",
    chunksSent: "chunks sent",
    runNow: "Run now",
    colTime: "Time",
    colAction: "Action",
    colApproved: "Approved by",
    colLeft: "Left device",
    // traceability
    everythingLeft: "Everything that left this device",
    traceHint: "Content is never logged — only a digest and a byte count, so this page can't leak what it records.",
    colRoute: "Route",
    colPurpose: "Purpose",
    colDestination: "Destination",
    colDigest: "Digest",
    colBytes: "Bytes",
    noThirdParty: "No third-party routes",
    noThirdPartyHint: "Nothing was handed to another service to send on your behalf.",
  },
} as const;

/** Active Full UI locale (v1: English fixed), mirroring `t` above. */
export const tf = FULL_UI.en;

export type Locale = keyof typeof STRINGS;

/** Active locale (v1: English fixed; a settings-driven value in Phase 1). */
export const t = STRINGS.en;
