type BrandIconProps = {
  domain: string;
  name: string;
  size?: number;
  className?: string;
};

const LOCAL_BRAND_ICONS: Record<string, string> = {
  'slack.com': '/brand-icons/slack.png',
  'gmail.com': '/brand-icons/gmail.svg',
  'notion.so': '/brand-icons/notion.png',
  'calendar.google.com': '/brand-icons/google-calendar.png',
  'github.com': '/brand-icons/github.png',
  'anthropic.com': '/brand-icons/claude.svg',
  'cursor.com': '/brand-icons/cursor.svg',
  'openai.com': '/brand-icons/chatgpt.png',
  'tsukuba.ac.jp': '/brand-icons/tsukuba.ico',
};

/** Official artwork is stored locally so icons stay crisp and never depend on a third-party request. */
export function BrandIcon({ domain, name, size = 24, className = '' }: BrandIconProps) {
  return (
    <img
      src={LOCAL_BRAND_ICONS[domain] ?? `/api/brand-logo/${domain}`}
      alt={`${name} logo`}
      width={size}
      height={size}
      loading="lazy"
      className={`shrink-0 object-contain ${className}`}
    />
  );
}
