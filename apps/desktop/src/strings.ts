// UI copy catalog — kept out of component markup per CLAUDE.md ("文言はコードから分離し
// i18n-readyに保つ"). v1 ships English only; the catalog shape is the i18n seam. Brand rules
// (CLAUDE.md): no competitor/stack names, the only emoji is ⚔, no "AI-powered/second brain".

export const STRINGS = {
  en: {
    // ── onboarding (issue #6) ─────────────────────────────────────────────────
    // First run, in its own window. The order is deliberate — what it does, what it reads and
    // never keeps, then permission, then the rest. Nothing is asked for before the reason to
    // grant it has been given. The permission step reuses the `onboarding` block below (#46).
    obNext: "Continue",
    obBack: "Back",
    obSkip: "Skip for now",
    obStep: "Step {n} of {total}",

    obWelcomeTitle: "SHOGUN lives in the notch.",
    obWelcomeBody:
      "It watches the work you're already doing, works out what you owe and what you're waiting on, and puts the next move one click away — right under the notch.",
    obWelcomePoint1: "Nothing to file, tag or maintain.",
    obWelcomePoint2: "It gets useful by tomorrow morning, and better every night.",
    obWelcomeStart: "Set up SHOGUN",

    obReadsTitle: "What it reads, and what it never keeps.",
    obReadsBody:
      "SHOGUN reads the text of the window you're working in, through macOS Accessibility. That's it.",
    obReadsKeep1Title: "No screenshots by default.",
    obReadsKeep1Body:
      "Screen reading is text only. Images exist only if you turn on Visual recall — compressed, encrypted on this Mac, and auto-deleted at the age you choose.",
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

    obPermTitle: "Give SHOGUN the access it needs.",
    obPermBody:
      "Three Mac permissions power context, dictation, meetings, and Visual recall. Set them up together; SHOGUN checks quietly and updates here live.",
    obPermGranted: "All permissions ready",
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
      "First-layer connections talk to each service from this Mac. Read-only to begin with — you can add sending later, once you trust it.",
    obDraftStop: "Drafts only",
    obDraftStopBody: "Write replies and leave them in drafts. Nothing is ever sent for you.",
    obDraftStopLocked: "Turning this off needs your consent first — that lives in Settings.",
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

    // Analytics opt-out toggle (issue #61 / #28 — on by default, metadata only, never content)
    analyticsToggleLabel: "Share anonymous usage metrics to help improve SHOGUN",
    analyticsToggleDetail:
      "Event names and timings only — never your content, screen text, or API keys.",
    // Settings › Privacy — the toggle's permanent home (issue #99); onboarding shows it once,
    // this section is where a set-up user changes their mind later.
    privacy: "Privacy",

    // ── panel ─────────────────────────────────────────────────────────────────
    // live line
    reading: "reading",
    yourScreen: "your screen",
    // tracked-items toggle (opens the commitments / open-loops list)
    stateList: "Tracked items",
    // ⌃⌥N is the actual summon shortcut (shortcuts.json default); ⌥ alone is the draft tap.
    openPanel: "Open ShogunAI (⌃⌥N)",
    // welcome (expanded, empty thread)
    welcomeTitle: "What can I take off your plate?",
    welcomeSub: "Ask about your work, or tap ⌥ (Option) in any app to draft where you're typing.",
    noKey: "No key yet — add one in settings for real answers.",
    // composer
    ask: "Ask ShogunAI…",
    scribeTitle: "Scribe",
    scribeLabel: "How should this text change?",
    scribePlaceholder: "Describe the edit…",
    scribeSubmit: "Apply edit",
    scribeProcessing: "Applying edit…",
    scribeError: "Couldn't apply that edit",
    // ⌥-tap feedback. Every one of these used to look the same from the outside — nothing —
    // which made a rejected key and a broken shortcut indistinguishable.
    inlineDrafting: "Drafting…",
    inlineInserted: "Drafted",
    inlineNoField: "No editable field here",
    inlineKeyRejected: "Key rejected — check it in settings",
    inlineNoKey: "Add a key in settings to draft",
    inlineFailed: "Couldn't draft",
    // What the app could not set up at boot. These are not transient outcomes — nothing the user
    // does in the panel clears them, so unlike the ⌥-tap lines above they stay until fixed. Each
    // says which capability is off, because "something is wrong" is not actionable.
    healthNoMemory: "Memory is off — capture and search unavailable",
    // Issue #121: the store opened but stopped answering. Says "unreadable", never "empty" —
    // the whole point is that the user must not read a failure as an honest blank.
    healthMemoryDegraded: "Memory isn't responding — what's shown may be incomplete",
    healthNoAccess: "Grant Accessibility to use drafting and capture",
    healthNoModel: "Search is text-only until the local model is installed",
    healthFix: "Fix",
    // context actions (B-1) — the buttons above the composer. Levels are the product's own
    // permission language (L1 auto / L2 one-tap / L3 approval), shown as a small tag.
    actionsAria: "Context actions",
    actionDone: "Done",
    actionFailed: "Couldn't run it",
    actionRejected: "Not available on this plan",
    actionGone: "That moment passed — the screen moved on",
    actionQueued: "Waiting in Approvals — nothing sends without you",
    actionConfirmQ: "Run it?",
    actionConfirm: "Confirm",
    actionCancel: "Cancel",
    actionExpired: "The confirm window passed",
    // in-panel memory search (B-6) — press / to search
    searchAria: "Search memory",
    searchPlaceholder: "Search your memory…",
    searchOpen: "Search memory (/)",
    // Issue #121: a store failure is not "no matches" — saying so would tell the user their
    // memory is empty when it is merely unreachable.
    searchUnavailable: "Memory is unavailable right now — this isn't an empty result.",
    searchEmpty: "No matches yet.",
    searchHint: "Enter copies the top match · Esc closes",
    searchCopied: "Copied",
    // approvals badge on the settings gear — how many sends wait for explicit confirmation
    approvalsBadge: (n: number): string => `${n} waiting for your approval`,
    // settings
    settings: "Settings",
    settingsGeneral: "General",
    settingsGeneralHint: "Plan, approvals, startup, and appearance.",
    settingsMemory: "Memory",
    settingsMemorySectionHint: "Capture, recall, local access, and nightly review.",
    settingsVoice: "Voice",
    settingsVoiceHint: "Dictation, meetings, sound, and daily summaries.",
    settingsConnections: "Connections",
    settingsConnectionsHint: "Services, accounts, and local activity sources.",
    settingsIntelligence: "Intelligence",
    settingsIntelligenceHint: "Models, subscriptions, and personalization.",
    settingsControls: "Controls",
    settingsControlsHint: "Shortcuts, notch placement, and visibility.",
    settingsPrivacy: "Privacy",
    settingsPrivacyHint: "Keys, data boundaries, and memory reset.",
    settingsSectionNav: "Settings sections",
    // Earlier conversations. Kept out of the way by default: the panel is for asking something
    // now, not for reading back a transcript.
    history: "Earlier messages",
    // Pinning is the counterpart to opening on hover: unpinned, the panel withdraws the moment
    // your attention does.
    pin: "Keep open",
    unpin: "Let it withdraw when I look away",
    // In-panel hub — the brief, health, memory and logs, drawn inside the notch panel so nothing
    // routine needs a separate window (meetings and Visual Recall keep their own surfaces).
    overview: "Overview",
    hubFailed: "Couldn't read your context",
    // connections (first-layer integrations)
    connections: "Connections",
    connectionsHint: "First-layer integrations connect directly to each service. Data stays on your device.",
    connectionsEmpty: "Loading services…",
    connectionsUnavailable: "Not available yet",
    connect: "Connect",
    connecting: "Connecting…",
    reconnect: "Reconnect",
    disconnect: "Disconnect",
    // AI coding-tool transcripts (opt-in source)
    aiSessions: "AI sessions",
    aiSessionsHint:
      "Read your local AI coding-tool transcripts so ShogunAI remembers what you worked on and decided there. Stays on your device.",
    aiSessionsOn: "Importing",
    aiSessionsOff: "Off",
    memoryApiTitle: "Memory API",
    memoryApiHint: "Let local tools read and update your memory over MCP, CLI, or loopback REST. Off by default.",
    memoryApiOn: "On",
    memoryApiOff: "Off",
    memoryApiProfileLabel: "Profile (whoami)",
    memoryApiDisplayName: "Display name",
    memoryApiRole: "Role",
    memoryApiPrefsHint: "Standing preferences local agents should follow.",
    memoryApiPrefsPlaceholder: "Prefer short answers. Never invent deadlines…",
    memoryApiSaveProfile: "Save profile",
    memoryApiTokensLabel: "API tokens",
    memoryApiTokensHint: "Issue one token per client. It is shown once, then only its verifier stays in Keychain.",
    memoryApiTokenNamePlaceholder: "Client name",
    memoryApiIssueToken: "Issue",
    memoryApiRevokeToken: "Revoke",
    memoryApiTokenIssued: "Copy token now. It will not be shown again.",
    memoryApiNoTokens: "No tokens yet. MCP keeps process trust until first token exists.",
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
      "Powers nightly review, meeting summaries, and live translation. Paste the API key itself, not an encoded copy of it.",
    selectKkPresent: "Connected — batch features and translation can run.",
    selectKkAbsent: "Not set — nightly review runs locally only; translation and AI summaries need this key.",
    selectKkPlaceholder: "Paste the Select KK API key…",
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
    showInDock: "Show in Dock",
    showInDockOn: "On",
    showInDockOff: "Off",
    showInDockHint:
      "Show ShogunAI in the Dock. Off keeps menu-bar only.",
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
    draftShortcut: "Draft with ShogunAI",
    recallShortcut: "Visual recall",
    summonShortcut: "Show / hide overlay",
    quitShortcut: "Quit",
    shortcuts: "Shortcuts",
    change: "Change",
    recordHint: "Press keys… (Esc to cancel)",
    needModifier: "Include a modifier (⌃ ⌥ ⇧ ⌘).",
    shortcutHint:
      "Click a shortcut to change it — saved instantly, works everywhere. Draft and Visual recall also take modifier gestures: tap one modifier alone, or press both sides of a modifier together.",
    // Personalization (Shougun.md): the one editable file that shapes how SHOGUN writes and acts.
    // "settings file", not the format's name — the UI does not teach the user a file format.
    personalizationTitle: "Personalization",
    personalizationHint: "Shape SHOGUN with one plain-text settings file you can read and edit.",
    personalizationOk: "Read successfully",
    personalizationMissing: "Not created yet",
    personalizationError: (section: string, line: number): string =>
      `Couldn't read it — check ${section || "the file"}, line ${line}`,
    personalizationOpen: "Open in editor",
    personalizationReset: "Start from a sample",
    learnedTitle: "Learned",
    learnedHint:
      "From edits you made before sending. These shape drafts only — they never change what needs confirmation.",
    learnedEmpty: "Nothing learned yet. Edit a draft before you send and patterns show up here.",
    learnedEvidence: (n: number): string => (n === 1 ? "1 correction" : `${n} corrections`),
    learnedToggle: "Use this lesson",
    voiceTitle: "Voice",
    voiceSection: "Voice dialogue",
    voiceHint: "Hold the shortcut, speak, release — on-device speech into the focused field (or clipboard). Beta; off by default.",
    thinkingAria: "Thinking",
    someApp: "an app",
    voiceOn: "On",
    voiceOff: "Off",
    voiceShortcut: "Hold to talk",
    voiceMicrophone: "Input microphone",
    voiceMicrophoneDefault: "Follow Mac system input",
    voiceMicrophoneRefresh: "Refresh",
    voiceMicrophoneLoading: "Scanning…",
    voiceMicrophonePickerTitle: "Choose Input",
    voiceMicrophoneClose: "Close microphone picker",
    voiceMicrophoneDefaultHint: "Uses the current macOS input",
    voiceMicrophoneAvailable: "Available",
    voiceMicrophoneDisconnected: (name: string): string => `${name} — unavailable`,
    voiceMicrophoneDisconnectedHint: "Reconnect it or choose another input",
    voiceMicrophoneHint:
      "Used for your next dictation. If this microphone disconnects, dictation stops instead of switching inputs.",
    voiceMicrophoneUnavailable: "Couldn't load microphones.",
    voiceMicrophoneSaveFailed: "Couldn't save that microphone. Keeping the previous input.",
    voiceEditModel: "Dictation cleanup",
    voiceEditModelHint:
      "When connected, dictation text is sent to Groq for process-only formatting. If it is unavailable or rejected, SHOGUN inserts the local vocabulary correction, or the raw transcript when nothing matches.",
    voiceEditKey: "Groq API key",
    voiceEditKeyHint: "Stored in Keychain on this Mac. Used only for fast dictation cleanup.",
    voiceEditKeyPresent: "Connected — cleanup is on.",
    voiceEditKeyAbsent: "Not set — local vocabulary and raw transcript only.",
    voiceEditKeyPlaceholder: "Paste your Groq API key…",
    voiceVocabulary: "Vocabulary",
    voiceVocabularyHint:
      "Add names, product terms, and jargon. SHOGUN uses them as local exact corrections.",
    voiceVocabularyEgress: "Speech-provider vocabulary hints",
    voiceVocabularyEgressOff:
      "Off by default. Personal vocabulary stays on this Mac for exact correction. Built-in terms may still be used as speech hints.",
    voiceVocabularyEgressOn:
      "On. Eligible personal vocabulary terms are sent with audio as speech hints. Egress is recorded without vocabulary words or transcripts.",
    voiceVocabularyEgressConsent:
      "I allow SHOGUN to send eligible personal vocabulary terms to my speech provider as recognition hints.",
    voiceVocabularyEgressSaveError: "Couldn’t save vocabulary sharing. Your choice was not changed.",
    voiceVocabularyTermLabel: "Use this spelling",
    voiceVocabularyAdvanced: "Language, app, and priority",
    voiceVocabularyPlaceholder: "Correct spelling",
    voiceVocabularyAliasesPlaceholder: "Misheard forms, separated by commas (optional)",
    voiceVocabularyLocale: "Language",
    voiceVocabularyLocalePlaceholder: "Language, e.g. en-US (optional)",
    voiceVocabularyScope: "Applies in",
    voiceVocabularyScopeGlobal: "Every app",
    voiceVocabularyScopeBundle: "One app bundle",
    voiceVocabularyScopeSurface: "One SHOGUN surface",
    voiceVocabularyScopeRef: "Scope identifier",
    voiceVocabularyScopeRefPlaceholder: "Bundle ID or surface identifier",
    voiceVocabularyScopeRefRequired: "Enter the bundle ID or surface identifier for this scope.",
    voiceVocabularyPriority: "Priority",
    voiceVocabularyPriorityInvalid: "Priority must be a whole number.",
    voiceVocabularyAdd: "Add term",
    voiceVocabularySave: "Save changes",
    voiceVocabularyCancel: "Cancel",
    voiceVocabularyEdit: "Edit",
    voiceVocabularyEditAria: (term: string): string => `Edit ${term}`,
    voiceVocabularyRemoveAria: (term: string): string => `Remove ${term}`,
    voiceVocabularyEnabled: "Enabled",
    voiceVocabularyLoading: "Loading personal terms…",
    voiceVocabularyLoadError: "Couldn't load personal vocabulary.",
    voiceVocabularyRetry: "Retry",
    voiceVocabularyEmpty: "No personal terms yet. Add a spelling that speech often gets wrong.",
    notchStatus: "Notch status",
    notchStatusShow: "Show",
    notchStatusHide: "Hide",
    notchStatusHint: "Show current app and activity in idle notch. Hide keeps notch quiet until hover.",
    voiceListening: "Listening…",
    voiceHoldHint: "Release when done",
    voiceProcessing: "Transcribing…",
    voiceAnswer: "Answer",
    voiceCopy: "Copy",
    voiceClose: "Close",
    voiceError: "Couldn't capture",

    // ── sounds (issue #49) ────────────────────────────────────────────────────
    // The mic line is not a footnote: sounds going quiet during a call is the design working,
    // and without saying so it reads as a bug worth reporting.
    soundSection: "Sounds",
    soundHint:
      "A short cue when something needs you, and when something breaks. Never during automatic work.",
    soundOff: "Off",
    soundEssential: "Essential",
    soundFull: "Full",
    soundOffHint: "Nothing makes a sound.",
    soundEssentialHint: "Only a decision you owe, and a failure that costs you work.",
    soundFullHint: "Also confirmations, and things finishing.",
    soundStartup: "Play the startup sound",
    soundStartupHint: "Off by default — ShogunAI starts with your Mac, which isn't a moment you chose.",
    soundQuietHours: "Quiet hours",
    soundQuietFrom: "From",
    soundQuietTo: "To",
    soundMicNote: "Sounds are always muted while any app is using the microphone.",
    soundPreview: "Preview",
    soundPreviewMuted: "Muted right now — a microphone is in use.",

    // ── daily summaries (issue #10) ───────────────────────────────────────────
    // Delivered on presence, not by interrupt: the greeting is the whole notification.
    goodMorning: "Good morning",
    goodEvening: "Good evening",
    dsSection: "Daily summaries",
    dsHint:
      "A brief when you arrive in the morning, and a wrap when the day winds down — shown in the notch the next time you're here, never as an interruption.",
    dsMorning: "Morning brief",
    dsEvening: "Evening wrap",
    dsEveningFrom: "Evening from",
    dsToday: "Today",
    dsCommitments: "Commitments due",
    dsOpenLoops: "Open loops",
    dsWhatHappened: "What happened",
    dsOutcome: "Today's outcome",
    dsDone: "commitments done",
    dsLoopsClosed: "loops closed",
    dsAdopted: "actions adopted",
    dsStillOpen: "Still open",
    dsTomorrowFirst: "Tomorrow first",
    dsLooseEnds: "Loose ends",
    dsPossibly: "possibly",
    dsUpdated: "Updated",
    dsEmptyMorning: "Nothing on the books yet — the brief fills in as capture runs.",
    dsClose: "Done",
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
    // ── Plan & billing (issue #8) ────────────────────────────────────────────
    // This is the ShogunAI subscription — not to be confused with `sub*` below, which is the
    // assistant plan a user already pays someone else for (Issue #110).
    planTitle: "Plan & billing",
    planHint: "Every plan starts with a 7-day full trial. Cards, invoices and cancellation are handled by our payment provider in your browser — never in this window.",
    planTrial: "Trial — everything unlocked",
    planTrialExpired: "Trial ended",
    planStandard: "Standard",
    planPro: "Pro",
    planStatusLabel: "Status",
    planNextBilling: "Next billing",
    planEndsOn: "Ends on",
    planLastChecked: "Last checked",
    // Offline grace (FR-BIL-09). Says how long is left, because "offline" alone is not actionable.
    planOffline: "Working offline — {n} of {total} days used. Connect to keep your plan active.",
    planUpgrade: "Upgrade",
    planBuyStandardYear: "Standard — $49/mo, billed annually",
    planBuyStandardMonth: "Standard — $62/mo",
    planBuyProYear: "Pro — $99/mo, billed annually",
    planBuyProMonth: "Pro — $124/mo",
    planManage: "Manage subscription",
    planRefresh: "Refresh",
    planActivateTitle: "Have a licence key?",
    planActivateHint: "Paste the key from your purchase confirmation to activate this Mac.",
    planActivatePlaceholder: "shogun-XXXX-XXXX-XXXX-XXXX",
    planActivate: "Activate",
    planActivating: "Activating…",
    planActivated: "This Mac is activated.",
    planDeactivate: "Remove licence from this Mac",
    planCancelsAtPeriodEnd: "Cancelled — access continues until the date above.",
    planExpiredHint: "Your trial has ended. Local capture and search keep working; connected tools, nightly review and actions need a plan.",
    // Subscription delegation (Issue #110): run on the plan the user already pays for, so the
    // first-run path never demands a second, metered credential.
    subTitle: "Your subscription",
    // Says "no API key to set up", not "free": a plan's allowance for this is finite, and copy
    // that implies otherwise turns into a support complaint the first time it runs out.
    subHint: "Already paying for an assistant? Run on that plan instead — no API key to set up. Each plan includes an allowance for this.",
    subNone: "No assistant found on this Mac. Install one, or add an API key below.",
    subUse: "Use this",
    subInUse: "In use",
    subTest: "Test connection",
    subTesting: "Testing…",
    subRefresh: "Look again",
    // Per-delegate state lines. Deliberately say what to DO, not just what is wrong.
    subStateReady: "Ready — running on your plan.",
    subStateInstalled: "Found. Test the connection to confirm you're signed in.",
    subStateNeedsLogin: "Signed out. Open a terminal and run it once to sign in.",
    subStateRateLimited: "This plan's allowance is used up. It refreshes on the plan's cycle — add an API key below to keep working before then.",
    subStateNotInstalled: "Not installed.",
    subRunsOn: "Runs on",
    // The disclosure. A consumer plan is not the metered API path, and the difference is the
    // user's to accept — so this is an explicit opt-in, never a default.
    subConsentTitle: "Before using your plan",
    subConsentItem1: "Your prompt leaves this device through that assistant, on your own plan.",
    subConsentItem2: "A consumer plan's data handling is set by that vendor, not by ShogunAI.",
    subConsentItem3: "ShogunAI never reads or stores that account's sign-in — it stays with the tool you installed.",
    subConsentAccept: "I understand — use my plan",
    subConsentRevoke: "Stop using my plan",
    subConsentNeeded: "Accept the note above to run on your plan.",
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
    // Google Calendar / Drive use direct OAuth; Gmail remains Composio-only by product decision.
    googleOAuthTitle: "Google Calendar / Drive OAuth",
    googleOAuthHint: "Optional desktop-client settings. Stored in macOS Keychain and never shown. Changing them disconnects Calendar and Drive so you can reconnect safely.",
    googleOAuthClientId: "Google OAuth client ID",
    googleOAuthClientSecret: "Google OAuth client secret (optional)",
    googleOAuthConfigured: "Client ID saved",
    googleOAuthMissing: "No saved client — development environment fallback may still be available.",
    googleOAuthSecretPresent: "Client secret saved",
    googleOAuthSecretOptional: "No client secret saved — PKCE can use a public desktop client.",
    googleOAuthSave: "Save client",
    googleOAuthClear: "Clear client",
    quit: "Quit ShogunAI",
    quitTitle: "Quit ShogunAI",
    minimize: "Minimize to the notch",
    resizeHint: "Drag to resize",
    send: "Send",
    stop: "Stop",
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
    meetingTakeNotes: "Take Notes",
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
      "Off by default. When on, sustained mic use can offer notes even without a meeting window in front.",
    deepgramAsrKey: "Speech provider key",
    deepgramAsrHint: "Powers live transcription. Paste once — stored in Keychain on this Mac.",
    deepgramAsrPresent: "Connected — live transcription can run.",
    deepgramAsrAbsent: "Not set — typed notes still save; live transcription needs this key.",
    deepgramAsrPlaceholder: "Paste the speech provider API key…",
    visualRecallSection: "Visual recall",
    visualRecallHint:
      "Passive OCR reads the focused window when accessibility text is thin. Saved frames stay encrypted on this Mac, then purge at your selected age.",
    visualRecallOn: "On",
    visualRecallOff: "Off",
    visualRecallDisclosure:
      "Requires Screen Recording for the focused window. Compressed JPEGs stay in the encrypted memory database; nothing is uploaded. Automatic age deletion is always on.",
    visualRecallRetention: "Keep saved screens",
    visualRecallRetentionDays: (days: number) => `${days} day${days === 1 ? "" : "s"}`,
    visualRecallRetentionCustom: "Custom",
    visualRecallRetentionCustomHint: "Choose 1–3,650 days. Longer retention uses more disk.",
    visualRecallRetentionApply: "Apply",
    visualRecallStoragePending: "Storage estimate appears after at least two saved screens.",
    visualRecallStorageEstimate: (current: string, projected: string, days: number) =>
      `${current} used now · about ${projected} over ${days} day${days === 1 ? "" : "s"}, based on the last 24 hours.`,
    visualRecallStoragePaused: (limit: string) =>
      `New capture paused at ${limit}. Existing screens stay until their selected age expires.`,
    visualRecallStatusOff: "Off — passive OCR paused. You can still save a screen manually.",
    visualRecallStatusIdle:
      "On — waiting for a window that needs OCR (canvas apps, terminals, or thin accessibility text).",
    visualRecallStatusLive: (n: number, app: string, window: string) =>
      `On — last read ${n} chars from ${app}${window ? ` · ${window}` : ""}.`,
    visualRecallTimeline: "Saved screens",
    visualRecallTimelineEmpty: "No saved screens yet — turn on Visual recall to start the local timeline.",
    visualRecallDeleteFrame: "Delete",
    visualRecallDeleteConfirm: "Remove this screen?",
    visualRecallDeleteCancel: "Cancel",
    visualRecallBrowse: "Browse saved screens",
    visualRecallBrowseSub: "Open the encrypted local timeline",
    visualRecallScrubHint: "Scrub through saved screens",
    visualRecallShowText: "Show text",
    visualRecallHideText: "Hide text",
    visualRecallClose: "Close",
    // FR-MT-03 disclosure (2026-08-05): Deepgram Nova-3 processes audio for STT only; MIP opt-out;
    // SHOGUN never saves waveform/recording. Reuse these keys everywhere.
    meetingDisclosure:
      "Nothing joins your call. Approved meetings send audio temporarily to our speech provider for live transcription only — not for training (MIP opt-out). SHOGUN never saves recordings or waveforms; only transcript text and provenance stay here.",
    meetingDisclosureBrief:
      "Nothing joins your call. Speech provider processes audio temporarily for transcription — not for training. No audio file saved by SHOGUN.",
    meetingDisclosureRecap:
      "Audio was used for transcription only — not kept for training. SHOGUN saves transcript text and your notes, never a recording.",
    // live in-meeting overlay (issue #93)
    meetingModeTranscription: "Transcribe",
    meetingModeOneWay: "One-way Translation",
    meetingModeTwoWay: "Two-way Translation",
    meetingLangAuto: "Auto",
    meetingLangEnglish: "English",
    meetingLangJapanese: "Japanese",
    meetingLangArrow: "→",
    meetingLangSwap: "↔",
    meetingLiveEmpty: "Listening…",
    meetingCopyTranscript: "Copy transcript",
    meetingCopiedTranscript: "Copied",
    meetingCaptionsSettings: "Display Settings",
    meetingCloseCaptionsPanel: "Close captions",
    meetingDisplaySettings: "Display Settings",
    meetingDisplayText: "Text",
    meetingDisplayWeight: "Weight",
    meetingDisplaySplit: "Split",
    meetingDisplaySizeS: "S",
    meetingDisplaySizeM: "M",
    meetingDisplaySizeL: "L",
    meetingDisplayWeightLight: "Light",
    meetingDisplayWeightBold: "Bold",
    meetingDisplaySplitSide: "Side",
    meetingDisplaySplitStack: "Stack",
    meetingTranslateNeedsKey:
      "Translation needs the Select KK key in Settings → Nightly review. Transcription still works.",
    meetingTranslateKeyInvalid:
      "Select KK key was rejected — re-paste it in Settings → Nightly review (the key itself, not an encoded copy).",
    // Privacy & Security (issue #28). One home for the key, the data-use policy, and deletion.
    privacyTitle: "Privacy & Security",
    // Key card. The key never leaves the Keychain and is never shown back in plaintext — settled
    // state is a set/not-set indicator only (no last-4: the backend deliberately hands out no key
    // material). Reuses the model/provider picker and key entry moved here from the old key block.
    keyEncryptedNote:
      "Your key is encrypted in the macOS Keychain. No one — including our team — can read it in plaintext.",
    // Data-use policy card.
    policyNotTrained: "Not used for model training",
    policyLocalFirst: "Local-first",
    policyEncrypted: "Encrypted at rest and in transit",
    policyLink: "Read the full privacy policy",
    // Data deletion card. Local and immediate — nothing is sent anywhere.
    deleteTitle: "Delete data",
    deleteHint: "Removes captured data from this device. This can't be undone.",
    deleteLast1h: "Last hour",
    deleteLast24h: "Last 24 hours",
    deleteAll: "Delete everything & account",
    // Range-specific confirm — names the window ({range} = "Last hour" / "Last 24 hours").
    deleteConfirmRange: "Delete {range} from this device? This can't be undone.",
    deleteAllConfirm:
      "This deletes everything and removes your keys. Type DELETE to confirm.",
    deleteAllConfirmPlaceholder: "Type DELETE",
    deleteConfirmBtn: "Delete",
    deleteDone: "Deleted from this device.",
    // Anonymous usage card (Slice D). Opt-in — OFF by default, and never carries captured content.
    analyticsTitle: "Anonymous usage",
    meetingCloseNote: "Close canvas",
    meetingOpenNotes: "AI Canvas",
    meetingCloseCaptions: "Close captions",
    meetingOpenCaptions: "Captions",
    meetingMore: "More",
    meetingPause: "Pause",
    meetingResume: "Resume",
    meetingEndMeeting: "End meeting notes",
    // AI Canvas (Notes pill) — live rolling summary + chronological timeline during a meeting
    meetingAiCanvas: "AI Canvas",
    meetingCanvasListening: "Listening..",
    meetingCanvasPaused: "Paused",
    meetingCanvasLiveSummary: "Live Summary",
    meetingCanvasTimeline: "Timeline",
    meetingCanvasManage: "Manage",
    meetingCanvasOfficial: "Official",
    meetingCanvasDrag: "Move",
    meetingCanvasSummaryWaiting: "Listening for more of the meeting before summarizing…",
    meetingCanvasTimelineEmpty: "Timeline fills as topics emerge.",
    meetingCanvasSummaryUpdating: "Updating summary…",
    meetingCanvasSummaryNeedsKey:
      "Live Summary needs the Select KK key in Settings → Nightly review.",
    meetingCanvasSummaryFailed: "Couldn’t update the summary. Will retry as the meeting continues.",
    meetingDisplayOriginal: "Original text",
    meetingChatPlaceholder: "Please ask anything about meeting.",
    meetingChatNew: "New chat",
    meetingChatSend: "Send",
    meetingChatClose: "Close chat",
    meetingChatStub:
      "Chat needs your own API key in Settings. Ask again after adding one — answers use the live transcript as context.",
    meetingChatEmpty: "Ask about this meeting.",
    meetingOpenChat: "Chat",
    meetingCloseChat: "Close chat",
    // errors
    sources: "Sources",
    noAnswer: "(no response)",
    answerFailed: "Couldn't answer",
    answerStopped: "Stopped.",
    // first-run Accessibility permission guide (Issue #46). Reusable shape: every field is copy for
    // ONE permission, so a future Microphone/Screen guide reuses the same component with its own
    // block. Brand rules: the only emoji is ⚔, no competitor/stack names, plain language.
    onboarding: {
      brand: "SHOGUN",
      permissionsLabel: "Required Mac permissions",
      readyCount: "{n} of 3 ready",
      permissionReady: "Ready",
      permissionAction: "Allow",
      accessibilityTitle: "Accessibility",
      accessibilityDetail: "Read the active window and insert drafts where you type.",
      microphoneTitle: "Microphone",
      microphoneDetail: "Power dictation and meeting capture when you explicitly start them.",
      screenTitle: "Screen Recording",
      screenDetail: "Capture the focused window only when Visual recall is enabled.",
      privacyNote: "Status checks never prompt. SHOGUN asks only after you click Allow.",
      dragTitle: "Drag ShogunAI into System Settings",
      dragHint: "Or click this card to reopen the exact privacy pane.",
      dragAria: "Drag ShogunAI into the {permission} privacy list",
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
        "Reads text only — images only with Visual recall on (encrypted, finite retention, on-device).",
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
    // The notch hub folds Context Health + Traceability into one low-priority tab at the far
    // right (2026-08-09 decision); the Full UI window keeps them as separate panes.
    navSystem: "System",
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
    reviewInPanel: "Confirm or reject from the SHOGUN panel — Settings › Approvals",
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

/** A count with its plural form. English needs one/other; the shape is what other locales need
 *  more of, so counted copy is written as a form table from the start rather than `+ " apps"`. */
export function count(forms: { one: string; other: string }, n: number): string {
  return (n === 1 ? forms.one : forms.other).replace("{n}", String(n));
}

/** Active locale (v1: English fixed; a settings-driven value in Phase 1). */
export const t = STRINGS.en;
