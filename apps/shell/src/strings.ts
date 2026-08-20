/** Chrome copy. Pane bodies arrive from Rust (`ShellView`) so this file does not invent status. */

export const copy = {
  product: "ShogunAI",
  tagline: "Your AI has memory. Now it acts.",
  navToday: "Today",
  navHealth: "Context Health",
  navSources: "Sources",
  navMemory: "Memory",
  navActivity: "Activity",
  navTrace: "Traceability",
  navSettings: "Settings",
  groupContext: "CONTEXT",
  groupDid: "WHAT IT DID",
  todaySub: "Your brief, your schedule, and what to do about it.",
  healthSub: "What SHOGUN can see, and what to fix.",
  sourcesSub: "Where context comes from, and what is excluded.",
  memorySub: "People, commitments, and open loops — with provenance.",
  activitySub: "Runs, approvals, and the nightly review.",
  traceSub: "Every off-device chunk. Digest only — never the body.",
  settingsSub: "This PC. Appearance, launch, and where data lives.",
  appearance: "Appearance",
  appearanceAuto: "Match the system",
  appearanceDark: "Dark",
  appearanceLight: "Light",
  launchAtLogin: "Launch at sign-in",
  launchAtLoginHint: "Starts SHOGUN when you sign in. It does not restart after you Quit.",
  dataFolder: "App data",
  openFolder: "Open folder",
  secrets: "Secrets",
  closeBehavior: "Window",
  minimize: "Minimize",
  maximize: "Maximize",
  restore: "Restore",
  close: "Close",
  bootFailed: "Couldn't read system status",
} as const;

const BANNED = ["ai-powered", "revolutionary", "second brain", "electron", "tauri", "webview"];

export function chromeContainsBanned(text: string): boolean {
  const lower = text.toLowerCase();
  return BANNED.some((w) => lower.includes(w));
}
