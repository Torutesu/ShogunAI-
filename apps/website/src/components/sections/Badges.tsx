import Image from 'next/image';
import type { Dictionary } from '@/i18n/dictionaries';
import { logoUrl } from '@/lib/logo-dev';

/* Ecosystem strip — the tools ShogunAI plugs into. Distinct from the trust-bar
   marquee, which is about who the product is for. Both draw from Logo.dev so
   there is one logo source in the LP. */
const ECOSYSTEM = [
  { name: 'OpenAI', domain: 'openai.com' },
  { name: 'Anthropic', domain: 'anthropic.com' },
  { name: 'NVIDIA', domain: 'nvidia.com' },
  { name: 'Notion', domain: 'notion.so' },
  { name: 'Vercel', domain: 'vercel.com' },
] as const;

function AwardBanner({
  src,
  alt,
  width,
  height,
  sizes,
  priority,
}: {
  src: string;
  alt: string;
  width: number;
  height: number;
  sizes: string;
  priority?: boolean;
}) {
  return (
    <div className="flex min-h-[116px] items-center justify-center px-2 py-2 sm:min-h-[128px] sm:px-3">
      <div className="relative flex h-[68px] w-full items-center justify-center sm:h-[80px]">
        <Image
          src={src}
          alt={alt}
          width={width}
          height={height}
          sizes={sizes}
          priority={priority}
          className="h-full w-full object-contain"
        />
      </div>
    </div>
  );
}

function ProductHuntBadge({ top, brand, mark }: { top: string; brand: string; mark: string }) {
  return (
    <div className="flex min-h-[116px] items-center justify-center px-2 py-2 sm:min-h-[128px] sm:px-3">
      <div className="flex h-[68px] w-full items-center justify-center gap-3 rounded-[22px] border border-white/55 bg-white/42 px-5 shadow-[0_18px_50px_rgba(0,67,101,0.08)] backdrop-blur-md sm:h-[80px]">
        <span className="flex size-11 shrink-0 items-center justify-center rounded-full bg-[#da552f] font-display text-[24px] font-bold leading-none text-white sm:size-12 sm:text-[26px]">
          {mark}
        </span>
        <span className="text-left leading-tight">
          <span className="block text-[11px] font-medium text-muted sm:text-xs">{top}</span>
          <span className="block font-display text-[16px] font-semibold tracking-[-0.01em] text-ink sm:text-[18px]">
            {brand}
          </span>
        </span>
      </div>
    </div>
  );
}

function EcosystemLogoStrip() {
  return (
    <div className="col-span-full mt-1 rounded-[24px] border border-white/50 bg-white/42 px-4 py-4 shadow-[0_18px_50px_rgba(0,67,101,0.08)] backdrop-blur-md sm:px-5">
      <p className="text-center text-[11px] font-semibold uppercase tracking-[0.12em] text-[#2b6173]">Works with the tools you already use</p>
      <div className="mt-4 flex flex-wrap items-center justify-center gap-x-7 gap-y-4 sm:gap-x-9">
        {ECOSYSTEM.map((brand) => (
          <span key={brand.domain} className="group/mark inline-flex items-center gap-2.5 text-[15px] font-semibold tracking-tight text-[#2b6173]">
            <img
              src={logoUrl(brand.domain, { size: 64, format: 'png' })}
              alt=""
              width={26}
              height={26}
              loading="lazy"
              decoding="async"
              className="brand-mark size-[26px] rounded-[6px] object-contain"
            />
            {brand.name}
          </span>
        ))}
      </div>
    </div>
  );
}

export function Badges({ t }: { t: Dictionary }) {
  const ph = t.authority.items.find((b) => b.tone === 'ph');

  return (
    <div className="mt-9 grid w-full max-w-[1120px] grid-cols-1 gap-4 lg:grid-cols-3">
      <AwardBanner
        src="/badges/yc-rfs-hackathon-2026-transparent-v2.png"
        alt="Winner of YC RFS Hackathon 2026, presented by Transpose"
        width={2055}
        height={765}
        sizes="(max-width: 1024px) 92vw, 360px"
        priority
      />
      <AwardBanner
        src="/badges/nvidia-inception-program.png"
        alt="NVIDIA Inception Program member badge"
        width={1221}
        height={662}
        sizes="(max-width: 1024px) 92vw, 360px"
      />
      {ph ? <ProductHuntBadge top={ph.top} brand={ph.brand} mark={ph.mark} /> : <div />}
      <EcosystemLogoStrip />
    </div>
  );
}
