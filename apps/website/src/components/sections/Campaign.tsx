import Link from 'next/link';
import { ArrowRight, Sparkles } from 'lucide-react';
import type { Dictionary } from '@/i18n/dictionaries';

/** Slim capped-campaign announcement (spec §5): "up to $500K, earned by action". */
export function Campaign({ t }: { t: Dictionary }) {
  return (
    <div className="campaign-bar relative overflow-hidden border-b border-accent/25">
      {/* diagonal light sweep that periodically crosses the bar */}
      <span aria-hidden="true" className="campaign-shine" />
      <Link
        href="/rules"
        className="group container-x relative flex items-center justify-center gap-2 py-2.5 text-center text-[13px] font-medium text-ink transition-colors hover:text-accent-strong"
      >
        <Sparkles className="campaign-spark size-3.5 shrink-0 text-accent" />
        <span className="text-balance">{t.campaign.text}</span>
        <span className="hidden shrink-0 items-center gap-1 rounded-full bg-accent px-2.5 py-0.5 font-semibold text-white shadow-[0_2px_8px_rgba(0,76,252,0.3)] sm:inline-flex">
          {t.campaign.cta}
          <ArrowRight className="size-3.5 transition-transform group-hover:translate-x-0.5" />
        </span>
      </Link>
    </div>
  );
}
