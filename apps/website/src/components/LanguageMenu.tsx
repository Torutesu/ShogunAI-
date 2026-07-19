'use client';

import { Globe } from 'lucide-react';
import { useRouter } from 'next/navigation';
import { useState, useTransition } from 'react';
import { LOCALE_COOKIE, type Locale, locales, localeNames } from '@/i18n/config';
import { cn } from '@/lib/utils';

/** Globe-icon language switcher. Opens on hover/focus; 4 languages. */
export function LanguageMenu({ current, label }: { current: Locale; label: string }) {
  const router = useRouter();
  const [, startTransition] = useTransition();
  const [active, setActive] = useState<Locale>(current);

  function pick(l: Locale) {
    setActive(l);
    document.cookie = `${LOCALE_COOKIE}=${l}; path=/; max-age=31536000; samesite=lax`;
    startTransition(() => router.refresh());
  }

  return (
    <div className="group relative">
      <button
        type="button"
        aria-label={label}
        aria-haspopup="menu"
        className="flex size-9 items-center justify-center rounded-full border border-border text-muted transition-colors hover:border-ink/25 hover:text-ink group-focus-within:text-ink"
      >
        <Globe className="size-4 transition-transform duration-500 group-hover:rotate-[24deg]" />
      </button>
      {/* pt-2 bridges the gap so hover doesn't drop */}
      <div className="invisible absolute right-0 top-full z-50 pt-2 opacity-0 transition-all duration-150 group-hover:visible group-hover:opacity-100 group-focus-within:visible group-focus-within:opacity-100">
        <div className="min-w-[180px] rounded-2xl border border-border bg-surface p-2 shadow-[var(--shadow-float)]">
          {locales.map((l) => (
            <button
              key={l}
              type="button"
              role="menuitemradio"
              aria-checked={active === l}
              onClick={() => pick(l)}
              className={cn(
                'flex w-full items-center justify-between rounded-lg px-3 py-2 text-left text-[15px] transition-colors hover:bg-cloud',
                active === l ? 'font-semibold text-ink' : 'text-muted',
              )}
            >
              {localeNames[l]}
              {active === l && <span className="size-1.5 rounded-full bg-accent" />}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
