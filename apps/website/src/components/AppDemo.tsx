'use client';

import { useState } from 'react';

/** The real desktop Full UI, running in an iframe with its own stylesheet.
  * The frame around it is the Mac window chrome, so the demo reads as the app
  * on a Mac rather than a widget on a marketing page. */
export function AppDemo({
  label,
  title,
  hint,
  openLabel,
}: {
  label: string;
  title: string;
  hint: string;
  openLabel: string;
}) {
  const [loaded, setLoaded] = useState(false);

  return (
    <div className="mx-auto w-full max-w-[1120px]">
      <div className="overflow-hidden rounded-[18px] border border-border bg-[#1c1c1e] shadow-[0_40px_100px_rgba(9,11,12,0.28)]">
        <div className="flex items-center gap-3 border-b border-white/8 bg-[linear-gradient(180deg,#3a3a3c,#2c2c2e)] px-4 py-2.5">
          <span className="flex shrink-0 items-center gap-2">
            <span className="size-3 rounded-full bg-[#ff5f57]" />
            <span className="size-3 rounded-full bg-[#febc2e]" />
            <span className="size-3 rounded-full bg-[#28c840]" />
          </span>
          <span className="min-w-0 flex-1 truncate text-center text-[12px] font-medium text-white/70">{title}</span>
          <span className="hidden shrink-0 text-[11px] text-white/40 sm:block">{label}</span>
        </div>
        <div className="relative bg-[#0b0b0c]">
          {!loaded && <div className="absolute inset-0 animate-pulse bg-[#141416]" aria-hidden="true" />}
          <iframe
            src="/app-demo/index.html"
            title={title}
            loading="lazy"
            onLoad={() => setLoaded(true)}
            className="block h-[560px] w-full border-0 sm:h-[640px]"
          />
        </div>
      </div>
      <div className="mt-3 flex flex-wrap items-center justify-between gap-3">
        <p className="text-[13px] text-muted">{hint}</p>
        <a
          href="/app-demo/index.html"
          target="_blank"
          rel="noreferrer"
          className="text-[13px] font-semibold text-accent hover:text-accent-strong"
        >
          {openLabel}
        </a>
      </div>
    </div>
  );
}
