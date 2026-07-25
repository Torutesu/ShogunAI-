import Image from 'next/image';
import type { Dictionary } from '@/i18n/dictionaries';

const BRANDFETCH_CLIENT_ID = process.env.NEXT_PUBLIC_BRANDFETCH_CLIENT_ID?.trim() ?? '';
const BRAND_LOGOS = [
  { name: 'OpenAI', domain: 'openai.com', width: 116 },
  { name: 'Anthropic', domain: 'anthropic.com', width: 140 },
  { name: 'NVIDIA', domain: 'nvidia.com', width: 132 },
  { name: 'Notion', domain: 'notion.so', width: 112 },
  { name: 'Vercel', domain: 'vercel.com', width: 118 },
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

function brandfetchLogoUrl(domain: string) {
  if (!BRANDFETCH_CLIENT_ID) return null;
  return `https://cdn.brandfetch.io/${domain}/theme/dark/logo.svg?c=${encodeURIComponent(BRANDFETCH_CLIENT_ID)}`;
}

function BrandfetchLogoStrip() {
  if (!BRANDFETCH_CLIENT_ID) return null;

  return (
    <div className="col-span-full mt-1 rounded-[24px] border border-white/50 bg-white/42 px-4 py-4 shadow-[0_18px_50px_rgba(0,67,101,0.08)] backdrop-blur-md sm:px-5">
      <p className="text-center text-[11px] font-semibold uppercase tracking-[0.12em] text-[#2b6173]">Works with the tools you already use</p>
      <div className="mt-4 flex flex-wrap items-center justify-center gap-x-7 gap-y-4 sm:gap-x-9">
        {BRAND_LOGOS.map((logo) => {
          const src = brandfetchLogoUrl(logo.domain);
          if (!src) return null;
          return (
            <img
              key={logo.domain}
              src={src}
              alt={`${logo.name} logo`}
              width={logo.width}
              height={32}
              loading="lazy"
              className="h-7 w-auto opacity-78 grayscale transition duration-300 hover:opacity-100 hover:grayscale-0"
            />
          );
        })}
      </div>
    </div>
  );
}

export function Badges({ t }: { t: Dictionary }) {
  const ph = t.authority.items.find((b) => b.tone === 'ph');

  return (
    <div className="mt-9 grid w-full max-w-[1120px] grid-cols-1 gap-4 lg:grid-cols-3">
      <AwardBanner
        src="/badges/yc-rfs-hackathon-2026.png"
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
      <BrandfetchLogoStrip />
    </div>
  );
}
