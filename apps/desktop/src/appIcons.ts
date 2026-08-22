// App bundle → display label, segment tint, optional serviceIcons key.
import { SERVICE_ICONS, type ServiceIcon } from "./serviceIcons";

export interface AppInfo {
  label: string;
  color: string;
  serviceKey?: string;
}

// Segment hues: soft pastels / clear brand-adjacent (avoid muddy near-blacks).
const BUNDLES: Record<string, AppInfo> = {
  "com.hnc.Discord": { label: "Discord", color: "#8B95F5" },
  "com.spotify.client": { label: "Spotify", color: "#5ED99A" },
  "com.google.Chrome": { label: "Chrome", color: "#7AB3F5" },
  "com.google.Chrome.canary": { label: "Chrome", color: "#7AB3F5" },
  "com.apple.Safari": { label: "Safari", color: "#6BB8FF" },
  "com.apple.SafariTechnologyPreview": { label: "Safari", color: "#6BB8FF" },
  "com.microsoft.VSCode": { label: "Code", color: "#5BB8E8" },
  "com.todesktop.230313mzl4w4u92": { label: "Cursor", color: "#A78BFA" },
  // Slack's real bundle id (same one meeting detection and the connect offer key off) — the
  // desktop app has never shipped as "com.slack.Slack".
  "com.tinyspeck.slackmacgap": { label: "Slack", color: "#E8A0BF" },
  "com.apple.mail": { label: "Mail", color: "#6BA3FF" },
  "com.apple.finder": { label: "Finder", color: "#7DD3FC" },
  "com.apple.Terminal": { label: "Terminal", color: "#94A3B8" },
  "dev.warp.Warp-Stable": { label: "Warp", color: "#3D4450" },
  "dev.warp.Warp": { label: "Warp", color: "#3D4450" },
  "com.figma.Desktop": { label: "Figma", color: "#C4A1FF" },
  "notion.id": { label: "Notion", color: "#A8A29E", serviceKey: "notion" },
  "com.linear": { label: "Linear", color: "#9AA3E8", serviceKey: "linear" },
  "com.openai.chat": { label: "Chat", color: "#5ECFB0" },
  "company.thebrowser.Browser": { label: "Arc", color: "#FF8F75" },
  "com.apple.Notes": { label: "Notes", color: "#F5D76E" },
  "net.shinyfrog.bear": { label: "Bear", color: "#9EC9F0" },
  "net.shinyfrog.bear-alt": { label: "Bear", color: "#9EC9F0" },
  "com.apple.iCal": { label: "Calendar", color: "#FF8A80", serviceKey: "gcal" },
  "com.apple.MobileSMS": { label: "Messages", color: "#6EE7A0" },
  "com.apple.systempreferences": { label: "Settings", color: "#C4C4C8" },
  "com.github.GitHubClient": { label: "GitHub", color: "#9CA3AF", serviceKey: "github" },
  "us.zoom.xos": { label: "Zoom", color: "#7EC4FF" },
  "com.google.meet": { label: "Meet", color: "#4DB6A8" },
  "com.apple.FinalCut": { label: "Final Cut", color: "#C084FC" },
  "com.adobe.Photoshop": { label: "Photoshop", color: "#6EC1FF" },
  "com.apple.dt.Xcode": { label: "Xcode", color: "#6BA8F5" },
};

const FALLBACK_COLORS = [
  "#9EC9F0", "#F5C6AA", "#B8E0D2", "#E8B4D4", "#D4C4F0",
  "#F5E6A3", "#A8D5A2", "#F2B8A0", "#A8C5D4", "#D4B8A8",
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

/** Soft pastel fill for timeline segments (readable on dark scrub shell). */
export function segmentTint(color: string, active = false): string {
  const hex = color.replace("#", "");
  const n = parseInt(hex.length === 3 ? hex.split("").map((c) => c + c).join("") : hex, 16);
  const r = (n >> 16) & 255;
  const g = (n >> 8) & 255;
  const b = n & 255;
  // Lift toward white so brand hues stay distinct without muddying.
  const lift = active ? 0.22 : 0.38;
  const rr = Math.round(r + (255 - r) * lift);
  const gg = Math.round(g + (255 - g) * lift);
  const bb = Math.round(b + (255 - b) * lift);
  return `rgba(${rr}, ${gg}, ${bb}, ${active ? 0.95 : 0.78})`;
}
