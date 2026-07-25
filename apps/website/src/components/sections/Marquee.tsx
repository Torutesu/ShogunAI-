import type { Dictionary } from '@/i18n/dictionaries';

const BRANDFETCH_CLIENT_ID = process.env.NEXT_PUBLIC_BRANDFETCH_CLIENT_ID?.trim() ?? '';

const TOOL_LOGOS = [
  { name: 'Slack', domain: 'slack.com', width: 110 },
  { name: 'Gmail', domain: 'google.com', width: 112 },
  { name: 'Notion', domain: 'notion.so', width: 108 },
  { name: 'GitHub', domain: 'github.com', width: 114 },
  { name: 'ChatGPT', domain: 'openai.com', width: 112 },
  { name: 'Claude', domain: 'anthropic.com', width: 118 },
  { name: 'Linear', domain: 'linear.app', width: 112 },
  { name: 'Vercel', domain: 'vercel.com', width: 110 },
  { name: 'Perplexity', domain: 'perplexity.ai', width: 126 },
  { name: 'Figma', domain: 'figma.com', width: 108 },
] as const;

function brandfetchLogoUrl(domain: string) {
  if (!BRANDFETCH_CLIENT_ID) return null;
  return `https://cdn.brandfetch.io/${domain}/logo?c=${encodeURIComponent(BRANDFETCH_CLIENT_ID)}`;
}

function Track({ reverse }: { reverse?: boolean }) {
  const items = [...TOOL_LOGOS, ...TOOL_LOGOS];

  return (
    <div className={`marquee-track ${reverse ? 'rev' : ''}`}>
      {items.map((tool, index) => {
        const src = brandfetchLogoUrl(tool.domain);
        return (
          <span
            key={`${tool.domain}-${index}`}
            className="mx-2.5 inline-flex h-[68px] shrink-0 items-center gap-4 rounded-[22px] border border-white/8 bg-white/[0.045] px-5 shadow-[inset_0_1px_0_rgba(255,255,255,0.04)] backdrop-blur-sm"
            aria-hidden={index >= TOOL_LOGOS.length}
          >
            {src ? (
              <img
                src={src}
                alt={`${tool.name} logo`}
                width={tool.width}
                height={28}
                loading="lazy"
                className="h-7 w-auto object-contain"
              />
            ) : (
              <span className="font-display text-[18px] font-semibold tracking-[-0.02em] text-white">{tool.name}</span>
            )}
            <span className="text-[15px] font-medium tracking-[-0.01em] text-white/78">{tool.name}</span>
          </span>
        );
      })}
    </div>
  );
}

export function Marquee({ t }: { t: Dictionary }) {
  return (
    <section className="py-8 sm:py-10">
      <div className="container-x">
        <div className="rounded-[34px] border border-white/9 bg-[#111111] px-4 py-4 shadow-[0_20px_80px_rgba(0,0,0,0.18),inset_0_1px_0_rgba(255,255,255,0.03)] sm:px-6 sm:py-5">
          <div className="mb-4 flex items-center gap-4 px-2 sm:px-3">
            <p className="shrink-0 text-[11px] font-semibold uppercase tracking-[0.34em] text-white/45">{t.trust.label}</p>
            <div className="h-px flex-1 bg-white/8" />
          </div>
          <div className="group/mq marquee-mask overflow-hidden">
            <Track />
          </div>
        </div>
      </div>
    </section>
  );
}
