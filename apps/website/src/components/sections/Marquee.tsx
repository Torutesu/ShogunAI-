import type { Dictionary } from '@/i18n/dictionaries';
import { BrandIcon } from '@/components/BrandIcon';

const BRAND_LOGOS = [
  { name: 'Linear', domain: 'linear.app', width: 110 },
  { name: 'Vercel', domain: 'vercel.com', width: 116 },
  { name: 'Perplexity', domain: 'perplexity.ai', width: 132 },
  { name: 'Figma', domain: 'figma.com', width: 104 },
  { name: 'Slack', domain: 'slack.com', width: 106 },
  { name: 'Gmail', domain: 'google.com', width: 108 },
  { name: 'Notion', domain: 'notion.so', width: 108 },
  { name: 'GitHub', domain: 'github.com', width: 114 },
  { name: 'ChatGPT', domain: 'openai.com', width: 114 },
  { name: 'Claude', domain: 'anthropic.com', width: 118 },
  { name: 'Discord', domain: 'discord.com', width: 116 },
  { name: 'Dropbox', domain: 'dropbox.com', width: 116 },
  { name: 'Asana', domain: 'asana.com', width: 104 },
  { name: 'Airbnb', domain: 'airbnb.com', width: 112 },
  { name: 'Stripe', domain: 'stripe.com', width: 96 },
  { name: 'Ramp', domain: 'ramp.com', width: 94 },
  { name: 'Salesforce', domain: 'salesforce.com', width: 116 },
  { name: 'IBM', domain: 'ibm.com', width: 82 },
  { name: 'Grok', domain: 'grok.com', width: 92 },
  { name: 'SpaceX', domain: 'spacex.com', width: 118 },
  { name: 'Paper', domain: 'paper.co', width: 96 },
  { name: 'Instagram', domain: 'instagram.com', width: 116 },
  { name: 'Y Combinator', domain: 'ycombinator.com', width: 132 },
  { name: 'Dia', domain: 'diabrowser.com', width: 88 },
  { name: 'Nike', domain: 'nike.com', width: 82 },
  { name: 'Harvard University', domain: 'harvard.edu', width: 136 },
  { name: 'MIT', domain: 'mit.edu', width: 84 },
  { name: 'UCLA', domain: 'ucla.edu', width: 96 },
  { name: 'University of British Columbia', domain: 'ubc.ca', width: 138 },
  { name: 'The University of Tokyo', domain: 'u-tokyo.ac.jp', width: 138 },
  { name: 'University of Tsukuba', domain: 'tsukuba.ac.jp', width: 138 },
  { name: 'Sequoia Capital', domain: 'sequoiacap.com', width: 132 },
  { name: 'a16z', domain: 'a16z.com', width: 82 },
  { name: 'Founders Fund', domain: 'foundersfund.com', width: 124 },
  { name: 'Accel', domain: 'accel.com', width: 84 },
  { name: 'Rippling', domain: 'rippling.com', width: 104 },
  { name: 'Coinbase', domain: 'coinbase.com', width: 108 },
  { name: 'cmux', domain: 'cmux.com', width: 84 },
  { name: 'Granola', domain: 'granola.ai', width: 96 },
  { name: 'Glean', domain: 'glean.com', width: 84 },
  { name: 'Gong', domain: 'gong.io', width: 82 },
  { name: 'Setlog', domain: 'setlog.com', width: 96 },
  { name: 'McKinsey & Company', domain: 'mckinsey.com', width: 132 },
  { name: 'Apple', domain: 'apple.com', width: 82 },
  { name: 'Cloudflare', domain: 'cloudflare.com', width: 104 },
  { name: 'THE SEED', domain: 'theseed.vc', width: 104 },
] as const;

const BRAND_ROWS = [
  BRAND_LOGOS.filter((_, index) => index % 2 === 0),
  BRAND_LOGOS.filter((_, index) => index % 2 === 1),
] as const;

function Track({ brands, reverse = false }: { brands: readonly (typeof BRAND_LOGOS)[number][]; reverse?: boolean }) {
  // Three identical sets keep the viewport filled at wide breakpoints and
  // give the CSS animation a complete, seamless cycle.
  const items = Array.from({ length: 3 }, () => brands).flat();

  return (
    <div className={`marquee-track ${reverse ? 'rev' : ''}`}>
      {items.map((tool, index) => {
        return (
          <span
            key={`${tool.domain}-${index}`}
            className="mx-6 inline-flex h-[42px] w-10 shrink-0 items-center justify-center sm:mx-7"
            aria-hidden={index >= brands.length}
          >
            <BrandIcon domain={tool.domain} name={tool.name} size={30} className="size-[30px] rounded-[7px]" />
          </span>
        );
      })}
    </div>
  );
}

export function Marquee({ t }: { t: Dictionary }) {
  return (
    // Its own band: hairlines top and bottom and a surface fill, so the strip
    // reads as a section between the hero and what follows instead of drifting
    // into the next one.
    <section
      aria-label={t.trust.label}
      className="marquee-band overflow-hidden border-y border-border bg-surface py-7 sm:py-8"
    >
      <div className="container-x">
        <div className="mb-4 flex items-center gap-5">
          <p className="shrink-0 text-[11px] font-semibold uppercase tracking-[0.14em] text-muted">{t.trust.label}</p>
          <div className="h-px flex-1 bg-border" />
        </div>
      </div>
      <div className="group/mq marquee-mask overflow-hidden">
        <Track brands={BRAND_ROWS[0]} />
        <div className="container-x">
          <div className="h-px bg-border/70" />
        </div>
        <Track brands={BRAND_ROWS[1]} reverse />
      </div>
    </section>
  );
}
