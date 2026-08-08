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
    openPanel: "Open ShogunAI (⌥J)",
    // welcome (expanded, empty thread)
    welcomeTitle: "What can I take off your plate?",
    welcomeSub: "Ask about your work, or tap ⌥ (Option) in any app to draft where you're typing.",
    noKey: "No key yet — add one in settings for real answers.",
    // composer
    ask: "Ask ShogunAI…",
    draftTitle: "Draft where you're typing (tap ⌥)",
    // ⌥-tap feedback. Every one of these used to look the same from the outside — nothing —
    // which made a rejected key and a broken shortcut indistinguishable.
    inlineDrafting: "Drafting…",
    inlineInserted: "Drafted",
    inlineNoField: "No editable field here",
    inlineKeyRejected: "Key rejected — check it in settings",
    inlineNoKey: "Add a key in settings to draft",
    inlineFailed: "Couldn't draft",
    // settings
    settings: "Settings",
    // Earlier conversations. Kept out of the way by default: the panel is for asking something
    // now, not for reading back a transcript.
    history: "Earlier messages",
    // Pinning is the counterpart to opening on hover: unpinned, the panel withdraws the moment
    // your attention does.
    pin: "Keep open",
    unpin: "Let it withdraw when I look away",
    // The separate window (spec §D) — where the brief, health, memory and logs live.
    openFullUi: "Open ShogunAI window",
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
      "Read your local AI coding-tool transcripts so ShogunAI remembers what you worked on and decided there. Stays on your device.",
    aiSessionsOn: "Importing",
    aiSessionsOff: "Off",
    // nightly cycle — what ShogunAI works out while you're away
    dream: "Nightly review",
    dreamHint: "While your Mac is idle overnight, ShogunAI works through the day and updates what it's tracking.",
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
    selectKkKey: "Select KK key",
    selectKkHint:
      "Powers nightly review, meeting summaries, and live translation. Paste the plain sk-ant-… API key — not hex.",
    selectKkPresent: "Connected — batch features and translation can run.",
    selectKkAbsent: "Not set — nightly review runs locally only; translation and AI summaries need this key.",
    selectKkPlaceholder: "Paste Select KK API key (sk-ant-…)…",
    selectKkSaved: "Select KK key saved.",
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
    // Castle Position (issue #20) — where SHOGUN resides on screen and expands from. "Castle"
    // because it's where SHOGUN keeps watch from; six resting places, the notch by default.
    castle: "Castle Position",
    castleHint:
      "Where SHOGUN lives on screen. Edges rest as a thin line, corners as a small box; hover to expand from wherever it's anchored.",
    castleNotch: "Notch",
    castleLeftEdge: "Left edge",
    castleRightEdge: "Right edge",
    castleBottomLeft: "Bottom left",
    castleBottomCenter: "Bottom",
    castleBottomRight: "Bottom right",
    behavior: "When you look away",
    launchAtLoginSection: "Launch at login",
    launchAtLoginOn: "On",
    launchAtLoginOff: "Off",
    launchAtLoginHint:
      "Open Shogun when you sign in to this Mac. Quitting stays quit until the next login.",
    stayOpen: "Stay open",
    autoHide: "Auto-hide",
    stayOpenHint: "Keep the panel open until you close it.",
    autoHideHint: "Slide back to the notch when you move away.",
    draftShortcut: "Draft with ShogunAI",
    draftFixedHint: "Tap ⌥ (Option) alone — always on, not rebindable.",
    summonShortcut: "Show / hide overlay",
    quitShortcut: "Quit",
    shortcuts: "Shortcuts",
    change: "Change",
    recordHint: "Press keys… (Esc to cancel)",
    needModifier: "Include a modifier (⌃ ⌥ ⇧ ⌘).",
    shortcutHint: "Click a shortcut to change it. Saved instantly, works everywhere.",
    voiceTitle: "Voice",
    voiceSection: "Voice dialogue",
    voiceHint: "Hold the shortcut, speak, release — on-device speech into the focused field (or clipboard). Beta; off by default.",
    voiceOn: "On",
    voiceOff: "Off",
    voiceShortcut: "Hold to talk",
    voiceListening: "Listening…",
    voiceHoldHint: "Release when done",
    voiceProcessing: "Transcribing…",
    voiceAnswer: "Answer",
    voiceCopy: "Copy",
    voiceClose: "Close",
    voiceError: "Couldn't capture",
    voicePasted: "Pasted",
    voiceCopied: "Copied to clipboard",
    voiceConflict: "Voice is unavailable while meeting notes are recording.",
    model: "Model",
    modelFor: "Runs on",
    modelHint: "Chat and ⌥-tap drafts run on this provider with your own key. Each provider keeps its own key below.",
    key: "Your key",
    keyPresent: "Connected — answers and drafts are yours.",
    keyAbsent: "Not set — ShogunAI will echo until you add a key.",
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
    // Composio sending (opt-in second-layer, FR-C2-02 / FR-C2-03)
    composioTitle: "Sending (Composio)",
    composioHint: "Optional. Enables sending email through Composio. Off by default — a key and explicit consent are required before any message leaves this device.",
    composioKeyAbsent: "No key",
    composioKeyPresent: "Key added",
    composioKeyPlaceholder: "Paste your Composio API key…",
    composioConsentTitle: "Before enabling sending",
    composioConsentItem1: "Sends go through Composio, a third-party service.",
    composioConsentItem2: "Your recipient, subject, and message body leave this device.",
    composioConsentItem3: "You can turn this off at any time.",
    composioGrantConsent: "Grant consent",
    composioConsentGranted: "Consent granted",
    composioRevokeConsent: "Revoke",
    composioDraftStop: "Draft-only mode (save a draft instead of sending)",
    composioUserId: "Composio user ID",
    composioUserIdHint: "Your Composio account's user identifier for the connected Gmail account.",
    quit: "Quit ShogunAI",
    quitTitle: "Quit ShogunAI",
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
    memoryHint: "Removes the commitments and open loops ShogunAI extracted. Your captured history stays. This can't be undone.",
    memoryClear: "Clear extracted state",
    memoryConfirm: "Permanently delete {n} extracted items? Type CLEAR to confirm.",
    memoryConfirmPlaceholder: "Type CLEAR",
    memoryClearConfirm: "Delete",
    memoryCleared: "Cleared.",
    cancel: "Cancel",
    // meeting notes — the pill (FR-MT-08/09). "Notes", never "Recording": nothing is recorded,
    // and the word would promise a file that will never exist.
    meetingNotes: "Notes",
    meetingDetected: "Meeting detected",
    meetingTakeNotes: "Take notes",
    meetingNotesSaved: "Saved",
    meetingNotesFailed: "Couldn't save",
    meetingRecapTitle: "Meeting notes",
    meetingRecapNoNotes: "No notes added. Transcript text may still be saved on this Mac.",
    meetingRecapMinutes: "min",
    meetingRecapDone: "Done",
    // Minutes — the model-generated layer (MT4, FR-MT-19), shown on top of the degraded Recap
    // once the Batch lane produces it. Next actions are suggestions to confirm, never things the
    // app will do (invariant 4).
    meetingMinutesSummary: "Summary",
    meetingMinutesDecisions: "Decisions",
    meetingMinutesNextActions: "Next actions",
    meetingMinutesPending: "Preparing summary…",
    meetingMinutesNeedsKey:
      "AI summary needs the Select KK key in Settings → Nightly review. Your transcript is shown below.",
    meetingRecapYourNotes: "Your notes",
    meetingTranscriptHeading: "Transcript",
    meetingTranscriptEmpty: "No transcript captured — model missing or silence.",
    meetingTranscriptOnlyBlanks:
      "Audio was heard but nothing clear was transcribed — only silence markers.",
    meetingTranscriptSpeakerMe: "Me",
    meetingTranscriptSpeakerOther: "Other",
    meetingTranscriptSpeakerUnknown: "Speaker",
    meetingUntitled: "Meeting",
    meetingStarting: "Starts in",
    meetingStart: "Start",
    meetingNotNow: "Not now",
    meetingStop: "Stop",
    meetingNotePlaceholder: "Type your notes…",
    meetingNeverThisApp: "Never for this app",
    // meeting notes — settings (FR-MT-01/02/03)
    meetingSection: "Meeting notes",
    meetingHint:
      "Detects meetings and transcribes on this Mac when you approve. One tap to decline, one tap to stop.",
    meetingOn: "On",
    meetingOff: "Off",
    meetingExcluded: "Never offer for",
    meetingExcludedEmpty: "No apps excluded",
    meetingExcludedRemove: "Remove",
    meetingMicOnly: "Detect by microphone alone",
    meetingMicOnlyHint:
      "Off by default. When on, sustained microphone use can offer notes without Meet or Zoom in front — useful for calls in other apps.",
    deepgramAsrKey: "Speech provider key",
    deepgramAsrHint:
      "Powers live meeting transcription. Paste once — stored in your Keychain on this Mac.",
    deepgramAsrPresent: "Connected — live transcription can run.",
    deepgramAsrAbsent:
      "Not set — your typed notes still save; live speech transcription needs this key.",
    deepgramAsrPlaceholder: "Paste Deepgram API key…",
    visualRecallSection: "Visual recall",
    visualRecallHint:
      "Passive OCR reads the focused window when accessibility text is thin. Saved frames stay on this Mac for up to 72 hours, then purge automatically.",
    visualRecallOn: "On",
    visualRecallOff: "Off",
    visualRecallDisclosure:
      "Requires Screen Recording for the focused window. Compressed JPEGs and OCR text stay local in memory for up to 72 hours — nothing is uploaded.",
    visualRecallStatusOff: "Off — passive OCR paused. You can still save a screen manually.",
    visualRecallStatusIdle:
      "On — waiting for a window that needs OCR (canvas apps, terminals, or thin accessibility text).",
    visualRecallStatusLive: (n: number, app: string, window: string) =>
      `On — last read ${n} chars from ${app}${window ? ` · ${window}` : ""}.`,
    visualRecallTimeline: "Saved screens (72 h)",
    visualRecallTimelineEmpty: "No saved screens yet — turn on Visual recall to start the local timeline.",
    visualRecallDeleteFrame: "Delete",
    visualRecallDeleteConfirm: "Remove this screen?",
    visualRecallDeleteCancel: "Cancel",
    visualRecallBrowse: "Browse saved screens",
    visualRecallBrowseSub: "Open the 72 h timeline on this Mac",
    visualRecallScrubHint: "Scrub through saved screens",
    visualRecallShowText: "Show text",
    visualRecallHideText: "Hide text",
    visualRecallClose: "Close",
    visualRecallSearchAll: "Search All",
    visualRecallSearchShortcut: "/",
    visualRecallSettings: "Settings",
    visualRecallZoomIn: "Zoom in",
    visualRecallZoomOut: "Zoom out",
    visualRecallOcrMode: "Show text overlay",
    visualRecallOcrModeOff: "Hide text overlay",
    visualRecallJumpToDay: "Jump to day",
    visualRecallNoFramesThatDay: "No screens on this day",
    visualRecallSearchEmpty: "No screens match your search",
    visualRecallRetentionNote: "Screens purge automatically after 72 hours.",
    visualRecallFrameMeta: (when: string, app: string, window: string) =>
      `${when} · ${app}${window ? ` · ${window}` : ""}`,
    // FR-MT-03 disclosure (2026-08-05): Deepgram Nova-3 processes audio for STT only; MIP opt-out;
    // SHOGUN never saves waveform/recording. Reuse these keys everywhere.
    meetingDisclosure:
      "Nothing joins your call. For meetings you approve, audio is temporarily processed by our speech provider for live transcription only — not retained for training (MIP opt-out). No recording, waveform, or audio file is saved by SHOGUN to disk or temp storage. Only transcript text and its provenance are kept here. Turn meetings off or exclude apps anytime below.",
    meetingDisclosureBrief:
      "Nothing joins your call. Speech provider processes audio temporarily for transcription — not for training. No audio file saved by SHOGUN.",
    meetingListening: "Listening · speech provider (not for training) · no recording saved",
    meetingDisclosureRecap:
      "Audio was processed temporarily for transcription only — not retained for training. No recording saved by SHOGUN — transcript, provenance, and your notes.",
    // live in-meeting overlay (issue #93)
    meetingModeTranscription: "Transcription",
    meetingModeOneWay: "One-way translation",
    meetingModeTwoWay: "Two-way translation",
    meetingLangAuto: "Auto-detect",
    meetingLangEnglish: "English",
    meetingLangJapanese: "Japanese",
    meetingLangArrow: "→",
    meetingLangSwap: "↔",
    meetingLiveEmpty: "Listening…",
    meetingLiveShowSource: "Show source",
    meetingLiveHideSource: "Hide source",
    meetingTranslateNeedsKey:
      "Translation needs the Select KK key in Settings → Nightly review. Transcription still works.",
    meetingTranslateKeyInvalid:
      "Select KK key was rejected — re-paste it in Settings → Nightly review (plain sk-ant-… key, not hex).",
    meetingOverlayClose: "Close",
    meetingOverlaySettings: "Settings",
    // errors
    sources: "Sources",
    noAnswer: "(no response)",
    answerFailed: "Couldn't answer",
    // first-run Accessibility permission guide (Issue #46). Reusable shape: every field is copy for
    // ONE permission, so a future Microphone/Screen guide reuses the same component with its own
    // block. Brand rules: the only emoji is ⚔, no competitor/stack names, plain language.
    onboarding: {
      brand: "SHOGUN",
      // guide (permission missing)
      guideTitle: "Let SHOGUN read your screen",
      guideLead:
        "SHOGUN builds your work context from on-screen text through macOS Accessibility. It reads text only — never images, never your keystrokes — and the context is built on this Mac.",
      doTitle: "What this turns on",
      doItems: [
        "Draft right where you're typing — tap ⌥ (Option) in any app.",
        "Open the notch panel and act on what you're looking at.",
        "Answer from the memory it builds of your work.",
      ],
      wontTitle: "What it never does",
      wontItems: [
        "Never captures images or screenshots.",
        "Never logs your keystrokes.",
        "Never sends your screen off this Mac.",
      ],
      stepsTitle: "Grant it in System Settings",
      steps: [
        "Open Privacy & Security › Accessibility.",
        "Find SHOGUN in the list.",
        "Turn the SHOGUN toggle on.",
        "Authenticate with your password or Touch ID.",
      ],
      cta: "Open Accessibility Settings",
      ctaAgain: "Open Settings again",
      skip: "Do this later",
      skipNote: "Until you grant it, drafting, the notch panel, and memory stay off. You can turn it on any time from Settings.",
      waiting: "Waiting for permission…",
      checking: "Checking…",
      notGranted: "Not granted yet",
      granted: "Granted",
      // troubleshooting (shown after the user has opened Settings at least once)
      troubleTitle: "Don't see SHOGUN, or the toggle won't stick?",
      troubleItems: [
        "If SHOGUN isn't in the list, reopen Settings — opening it adds SHOGUN to Accessibility.",
        "If the toggle is on but nothing works, quit and reopen SHOGUN.",
        "After reinstalling, remove the old SHOGUN row, then add it again.",
      ],
      // success (permission granted)
      successTitle: "You're set",
      successLead:
        "SHOGUN can read your screen now. Reach it two ways: tap ⌥ (Option) in any app, or rest your cursor on the notch.",
      successCta: "Open SHOGUN",
    },
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
    healthSub: "What ShogunAI can and can't see right now — every number has a way to fix it.",
    sourcesSub: "Where context comes from, how fresh it is, and what's excluded.",
    memorySub: "Everything ShogunAI has extracted, with the evidence behind it.",
    activitySub: "What ran, at what level, and whether anything left.",
    traceSub: "Every byte that left this device — digest and size only, never content.",
    // health
    confidenceMix: "Confidence mix",
    slo: "Latency & CPU",
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
    sourcesHint: "Each one connects directly. Scope is what ShogunAI is allowed to read.",
    exclusions: "Capture exclusions",
    exclusionsHint: "Apps and windows ShogunAI never reads. Excluded time still counts against coverage.",
    alwaysExcluded: "Always excluded",
    healthy: "Healthy",
    needsAttention: "Needs attention",
    thirdParty: "third-party",
    direct: "Direct",
    on: "On",
    off: "Off",
    // memory
    commitments: "Commitments",
    commitmentsHint: "Every row carries the evidence it came from and how sure ShogunAI is.",
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
    // Empty states. Each says what would be here and what produces it — a blank card reads as a
    // broken screen, and "No data" tells the user nothing they can act on.
    emptySources: "No services connected yet. Connect one and its context starts feeding the panel — read-only, and only what you allow.",
    emptyCommitments: "Nothing extracted yet. Commitments appear here once the nightly review has worked through a day of captured context.",
    emptyMerge: "No name collisions to resolve.",
    emptyPending: "Nothing waiting. Anything that would leave your device queues here first.",
    emptyTrace: "Nothing has left this device.",
    emptyBriefActions: "No suggestions yet — they come from the brief once it has run.",
    emptySchedule: "No calendar connected, so there's nothing scheduled to show.",
    emptyHealth: "Nothing measured yet. These fill in as capture runs and the nightly review completes.",
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
