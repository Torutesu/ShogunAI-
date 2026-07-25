import type { Dictionary } from '@/i18n/dictionaries';
import { logoUrl } from '@/lib/logo-dev';

/* Official brand marks served by Logo.dev, looked up by domain — the most
   stable identifier (name lookups can land on the wrong "Linear" or "Loom").
   Requested greyscale so the row keeps its single-tone trust-bar look; see
   `.brand-mark` in globals.css for the chip that carries them in both themes. */
type Brand = { name: string; domain: string };

const ROW_A: Brand[] = [
  { name: 'Slack', domain: 'slack.com' },
  { name: 'Notion', domain: 'notion.so' },
  { name: 'Linear', domain: 'linear.app' },
  { name: 'GitHub', domain: 'github.com' },
  { name: 'Figma', domain: 'figma.com' },
  { name: 'Airbnb', domain: 'airbnb.com' },
  { name: 'Coinbase', domain: 'coinbase.com' },
  { name: 'Dropbox', domain: 'dropbox.com' },
  { name: 'Instacart', domain: 'instacart.com' },
];

const ROW_B: Brand[] = [
  { name: 'Stripe', domain: 'stripe.com' },
  { name: 'Vercel', domain: 'vercel.com' },
  { name: 'Zoom', domain: 'zoom.us' },
  { name: 'Loom', domain: 'loom.com' },
  { name: 'Gmail', domain: 'gmail.com' },
  { name: 'DoorDash', domain: 'doordash.com' },
  { name: 'Reddit', domain: 'reddit.com' },
  { name: 'Docker', domain: 'docker.com' },
  { name: 'GitLab', domain: 'gitlab.com' },
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
          className="group/mark mx-7 inline-flex shrink-0 items-center gap-2.5 text-[19px] font-semibold tracking-tight text-faint transition-colors hover:text-muted"
          aria-hidden={i >= items.length}
        >
          {/* alt="" — the brand name is right there as text, so announcing the
              logo too would just double it up for screen readers. */}
          <img
            src={logoUrl(brand.domain, { size: SOURCE_PX, format: 'png', greyscale: true })}
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
