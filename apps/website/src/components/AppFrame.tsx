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
  label,
}: {
  src: string;
  title: string;
  className?: string;
  chrome?: boolean;
  label?: string;
}) {
  const [loaded, setLoaded] = useState(false);

  const frame = (
    <div className="relative h-full w-full">
      {!loaded && <div className="absolute inset-0 animate-pulse bg-[#141416]" aria-hidden="true" />}
      <iframe
        src={src}
        title={title}
        loading="lazy"
        onLoad={() => setLoaded(true)}
        className="block h-full w-full border-0"
      />
    </div>
  );

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
