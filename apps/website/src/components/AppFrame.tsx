'use client';

import { useState } from 'react';

/** A real desktop surface, running in an iframe with its own stylesheet.
  * `chrome` draws Mac window furniture around it — the notch panel brings its
  * own, the Full UI window does not. */
export function AppFrame({
  src,
  title,
  className = '',
  chrome = false,
  device,
  label,
}: {
  src: string;
  title: string;
  className?: string;
  chrome?: boolean;
  device?: 'mac';
  label?: string;
}) {
  const [loaded, setLoaded] = useState(false);

  const frame = (
    <div className="relative h-full w-full">
      {!loaded && <div className="absolute inset-0 animate-pulse bg-black/20" aria-hidden="true" />}
      <iframe
        src={src}
        title={title}
        loading="lazy"
        onLoad={() => setLoaded(true)}
        className="block h-full w-full border-0 bg-transparent"
      />
    </div>
  );

  if (device === 'mac') {
    return (
      <div className={className}>
        {/* Aluminium body → bezel → notch → wallpaper → the panel hanging from it.
          * The panel document is transparent, so the wallpaper behind the iframe is
          * what the panel appears to float over — which is where it lives on a Mac. */}
        <div className="rounded-[22px] bg-[linear-gradient(180deg,#d9dade,#b9bcc2)] p-[6px] shadow-[0_40px_90px_rgba(9,11,12,0.26)]">
          <div className="relative overflow-hidden rounded-[17px] bg-black p-[8px]">
            <div className="relative overflow-hidden rounded-[10px] bg-[radial-gradient(120%_120%_at_50%_0%,#4b5f8f_0%,#2b3556_42%,#171d31_100%)]">
              <div className="absolute left-1/2 top-0 z-20 h-[18px] w-[132px] -translate-x-1/2 rounded-b-[10px] bg-black" aria-hidden="true" />
              {/* The panel hangs from the notch rather than filling the display, so
                * it is inset and top-anchored and the wallpaper stays visible around it. */}
              <div className="relative h-[380px] sm:h-[420px]">
                <div className="mx-auto h-full w-[82%]">{frame}</div>
              </div>
            </div>
          </div>
        </div>
        <div className="mx-auto h-[10px] w-[86%] rounded-b-[10px] bg-[linear-gradient(180deg,#c3c6cc,#9aa0a8)]" aria-hidden="true" />
        <div className="mx-auto h-[4px] w-[30%] rounded-b-[6px] bg-[#8b9098]" aria-hidden="true" />
      </div>
    );
  }

  if (!chrome) return <div className={className}>{frame}</div>;

  return (
    <div className={`overflow-hidden rounded-[18px] border border-border bg-[#1c1c1e] shadow-[0_40px_100px_rgba(9,11,12,0.28)] ${className}`}>
      <div className="flex items-center gap-3 border-b border-white/8 bg-[linear-gradient(180deg,#3a3a3c,#2c2c2e)] px-4 py-2.5">
        <span className="flex shrink-0 items-center gap-2">
          <span className="size-3 rounded-full bg-[#ff5f57]" />
          <span className="size-3 rounded-full bg-[#febc2e]" />
          <span className="size-3 rounded-full bg-[#28c840]" />
        </span>
        <span className="min-w-0 flex-1 truncate text-center text-[12px] font-medium text-white/70">{title}</span>
        {label && <span className="hidden shrink-0 text-[11px] text-white/40 sm:block">{label}</span>}
      </div>
      <div className="h-[calc(100%-45px)] bg-[#0b0b0c]">{frame}</div>
    </div>
  );
}
