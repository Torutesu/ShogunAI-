// App bundle → display label, segment tint, optional serviceIcons key.
import { SERVICE_ICONS, type ServiceIcon } from "./serviceIcons";

export interface AppInfo {
  label: string;
  color: string;
  serviceKey?: string;
}

const BUNDLES: Record<string, AppInfo> = {
  "com.hnc.Discord": { label: "Discord", color: "#5865F2" },
  "com.spotify.client": { label: "Spotify", color: "#1DB954" },
  "com.google.Chrome": { label: "Chrome", color: "#4285F4" },
  "com.google.Chrome.canary": { label: "Chrome", color: "#4285F4" },
  "com.apple.Safari": { label: "Safari", color: "#0A84FF" },
  "com.apple.SafariTechnologyPreview": { label: "Safari", color: "#0A84FF" },
  "com.microsoft.VSCode": { label: "Code", color: "#007ACC" },
  "com.todesktop.230313mzl4w4u92": { label: "Cursor", color: "#7C3AED" },
  "com.slack.Slack": { label: "Slack", color: "#4A154B" },
  "com.apple.mail": { label: "Mail", color: "#007AFF" },
  "com.apple.finder": { label: "Finder", color: "#5AC8FA" },
  "com.apple.Terminal": { label: "Terminal", color: "#3C3C3C" },
  "com.figma.Desktop": { label: "Figma", color: "#A259FF" },
  "notion.id": { label: "Notion", color: "#000000", serviceKey: "notion" },
  "com.linear": { label: "Linear", color: "#5E6AD2", serviceKey: "linear" },
  "com.openai.chat": { label: "Chat", color: "#10A37F" },
  "company.thebrowser.Browser": { label: "Arc", color: "#FC5C3C" },
  "com.apple.Notes": { label: "Notes", color: "#FFCC00" },
  "com.apple.iCal": { label: "Calendar", color: "#FF3B30", serviceKey: "gcal" },
  "com.apple.MobileSMS": { label: "Messages", color: "#34C759" },
  "com.apple.systempreferences": { label: "Settings", color: "#8E8E93" },
  "com.github.GitHubClient": { label: "GitHub", color: "#181717", serviceKey: "github" },
  "us.zoom.xos": { label: "Zoom", color: "#2D8CFF" },
  "com.google.meet": { label: "Meet", color: "#00897B" },
  "com.apple.FinalCut": { label: "Final Cut", color: "#A855F7" },
  "com.adobe.Photoshop": { label: "Photoshop", color: "#31A8FF" },
  "com.apple.dt.Xcode": { label: "Xcode", color: "#147EFB" },
};

const FALLBACK_COLORS = [
  "#6366f1", "#8b5cf6", "#ec4899", "#f43f5e", "#f97316",
  "#eab308", "#22c55e", "#14b8a6", "#06b6d4", "#3b82f6",
];

function hashTint(key: string): string {
  let h = 0;
  for (let i = 0; i < key.length; i++) h = (h * 31 + key.charCodeAt(i)) >>> 0;
  return FALLBACK_COLORS[h % FALLBACK_COLORS.length]!;
}

/** Resolve bundle id (or display name fallback) to label + tint for timeline segments. */
export function resolveApp(bundleOrName: string | null | undefined): AppInfo {
  if (!bundleOrName) return { label: "Unknown", color: "#6b7280" };
  const direct = BUNDLES[bundleOrName];
  if (direct) return direct;
  const short = bundleOrName.split(".").pop() ?? bundleOrName;
  const label = short.length > 1 ? short.charAt(0).toUpperCase() + short.slice(1) : short;
  return { label, color: hashTint(bundleOrName) };
}

export function appServiceIcon(info: AppInfo): ServiceIcon | undefined {
  if (info.serviceKey) return SERVICE_ICONS[info.serviceKey];
  return undefined;
}

/** Segment tint at ~18% opacity for timeline bar. */
export function segmentTint(color: string): string {
  const hex = color.replace("#", "");
  const n = parseInt(hex.length === 3 ? hex.split("").map((c) => c + c).join("") : hex, 16);
  const r = (n >> 16) & 255;
  const g = (n >> 8) & 255;
  const b = n & 255;
  return `rgba(${r}, ${g}, ${b}, 0.55)`;
}
