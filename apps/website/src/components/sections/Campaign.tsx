import Link from 'next/link';
import { ArrowRight, Sparkles } from 'lucide-react';
import type { Dictionary } from '@/i18n/dictionaries';

/** Slim capped-campaign announcement (spec §5): "up to $500K, earned by action". */
export function Campaign({ t }: { t: Dictionary }) {
  return (
    <div className="border-b border-border bg-gradient-to-r from-sky-soft/70 via-cloud to-sky-soft/70">
      <Link
        href="/rules"
        className="group container-x flex items-center justify-center gap-2 py-2.5 text-center text-[13px] font-medium text-ink transition-colors hover:text-accent-strong"
      >
        <Sparkles className="size-3.5 shrink-0 text-accent" />
        <span className="text-balance">{t.campaign.text}</span>
        <span className="hidden shrink-0 items-center gap-0.5 font-semibold text-accent-strong sm:inline-flex">
          {t.campaign.cta}
          <ArrowRight className="size-3.5 transition-transform group-hover:translate-x-0.5" />
        </span>
      </Link>
    </div>
  );
}
