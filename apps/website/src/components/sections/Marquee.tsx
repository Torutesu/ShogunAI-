import type { Dictionary } from '@/i18n/dictionaries';
import { logoUrl } from '@/lib/logo-dev';

/* Official brand marks served by Logo.dev, looked up by domain — the most
   stable identifier (name lookups can land on the wrong "Linear" or "Loom").
   Served in colour and desaturated in CSS, so `.brand-mark` in globals.css can
   animate the reveal on hover off a single image. */
type Brand = { name: string; domain: string };

/* Top row: the AI and builder tooling the product lives next to. */
const ROW_A: Brand[] = [
  { name: 'OpenAI', domain: 'openai.com' },
  { name: 'Anthropic', domain: 'anthropic.com' },
  { name: 'Google DeepMind', domain: 'deepmind.google' },
  { name: 'Cursor', domain: 'cursor.com' },
  { name: 'Replit', domain: 'replit.com' },
  { name: 'GitHub', domain: 'github.com' },
  { name: 'Vercel', domain: 'vercel.com' },
  { name: 'Linear', domain: 'linear.app' },
  { name: 'Figma', domain: 'figma.com' },
  { name: 'Stripe', domain: 'stripe.com' },
  { name: 'Ramp', domain: 'ramp.com' },
  { name: 'Rippling', domain: 'rippling.com' },
  { name: 'Airtable', domain: 'airtable.com' },
];

/* Bottom row: the data stack and the household names. */
const ROW_B: Brand[] = [
  { name: 'Retool', domain: 'retool.com' },
  { name: 'PostHog', domain: 'posthog.com' },
  { name: 'Snowflake', domain: 'snowflake.com' },
  { name: 'Apple', domain: 'apple.com' },
  { name: 'NVIDIA', domain: 'nvidia.com' },
  { name: 'Meta', domain: 'meta.com' },
  { name: 'Microsoft', domain: 'microsoft.com' },
  { name: 'Google', domain: 'google.com' },
  { name: 'Amazon', domain: 'amazon.com' },
  { name: 'Netflix', domain: 'netflix.com' },
  { name: 'Airbnb', domain: 'airbnb.com' },
  { name: 'Shopify', domain: 'shopify.com' },
  { name: 'Tesla', domain: 'tesla.com' },
];

/* Rendered at 22px (see `size-[22px]` below); a 64px source keeps it crisp on
   retina without paying for the CDN's full 2x `retina` render. */
const SOURCE_PX = 64;

function Track({ items, reverse }: { items: Brand[]; reverse?: boolean }) {
  const doubled = [...items, ...items];
  return (
    <div className={`marquee-track ${reverse ? 'rev' : ''}`}>
      {doubled.map((brand, i) => (
        <span
          key={`${brand.name}-${i}`}
          className="group/mark mx-7 inline-flex shrink-0 items-center gap-2.5 text-[19px] font-semibold tracking-tight text-faint transition-colors hover:text-ink"
          aria-hidden={i >= items.length}
        >
          {/* alt="" — the brand name is right there as text, so announcing the
              logo too would just double it up for screen readers. */}
          <img
            src={logoUrl(brand.domain, { size: SOURCE_PX, format: 'png' })}
            alt=""
            width={22}
            height={22}
            loading="lazy"
            decoding="async"
            className="brand-mark size-[22px] rounded-[5px] object-contain"
          />
          {brand.name}
        </span>
      ))}
    </div>
  );
}

export function Marquee({ t }: { t: Dictionary }) {
  return (
    <section className="border-y border-border/60 py-12">
      <div className="container-x">
        <p className="mb-7 text-center text-xs font-medium tracking-[0.02em] text-muted">{t.trust.label}</p>
      </div>
      <div className="group/mq marquee-mask flex flex-col gap-5">
        <Track items={ROW_A} />
        <Track items={ROW_B} reverse />
      </div>
    </section>
  );
}
