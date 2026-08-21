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
  scaleClassName = '',
  widthClassName = '',
  href,
  linkLabel,
  surfaceClassName = '',
}: {
  src: string;
  alt: string;
  width: number;
  height: number;
  sizes: string;
  priority?: boolean;
  scaleClassName?: string;
  widthClassName?: string;
  href?: string;
  linkLabel?: string;
  surfaceClassName?: string;
}) {
  const banner = (
    <div className={`relative flex h-full w-full items-center justify-center ${surfaceClassName}`}>
      <Image
        src={src}
        alt={alt}
        width={width}
        height={height}
        sizes={sizes}
        priority={priority}
        className={`h-full w-auto max-w-full object-contain ${scaleClassName}`}
      />
    </div>
  );

  const content = <div className={`flex h-[64px] items-center justify-center sm:h-[96px] ${widthClassName}`}>{banner}</div>;

  return href ? (
    <a href={href} target="_blank" rel="noreferrer" aria-label={linkLabel ?? alt} className="block h-full rounded-[18px] transition-transform hover:-translate-y-0.5">
      {content}
    </a>
  ) : content;
}

function ProductHuntBadge() {
  return (
    <div className="flex h-[64px] items-center justify-center sm:h-[96px]">
      <a
        href="https://www.producthunt.com/products/shogunai?embed=true&utm_source=badge-featured&utm_medium=badge&utm_campaign=badge-shogunai"
        target="_blank"
        rel="noopener noreferrer"
        aria-label="View ShogunAI on Product Hunt"
        className="flex h-full items-center justify-center rounded-[18px] transition-transform hover:-translate-y-0.5"
      >
        <img
          src="https://api.producthunt.com/widgets/embed-image/v1/featured.svg?post_id=1228269&theme=light&t=1787296862710"
          alt="ShogunAI - Your personal AGI on your PC. Built to finish real work. | Product Hunt"
          width={250}
          height={54}
          className="h-full w-auto max-w-[248px] object-contain sm:max-w-[352px]"
        />
      </a>
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
    <div className="mx-auto grid w-full max-w-[1000px] grid-cols-3 items-center justify-items-center gap-x-6 gap-y-6 sm:gap-x-12">
      <AwardBanner
        src="/optimized/yc-rfs-hackathon-2026.png"
        alt="Winner of YC RFS Hackathon 2026, presented by Transpose"
        width={2055}
        height={765}
        sizes="(max-width: 1024px) 92vw, 420px"
        priority
        widthClassName="w-full max-w-[260px] sm:max-w-[360px]"
        href="https://x.com/toruai/status/2082832405514395962?s=20"
        linkLabel="Open the YC RFS Hackathon 2026 win announcement on X"
      />
      {ph ? <ProductHuntBadge /> : <div />}
      <AwardBanner
        src="/badges/nvidia-inception-program.png"
        alt="NVIDIA Inception Program member badge"
        width={1221}
        height={662}
        sizes="(max-width: 1024px) 92vw, 420px"
        widthClassName="w-full max-w-[190px] sm:max-w-[262px]"
        surfaceClassName="theme-light-badge"
      />
      <BrandfetchLogoStrip />
    </div>
  );
}
