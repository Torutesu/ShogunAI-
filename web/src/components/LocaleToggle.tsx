'use client';

import { useRouter } from 'next/navigation';
import { useState, useTransition } from 'react';
import { LOCALE_COOKIE, type Locale } from '@/i18n/config';
import { cn } from '@/lib/utils';

/** EN/JA switch. Persists the choice in a cookie and re-renders the tree. */
export function LocaleToggle({ locale, label }: { locale: Locale; label: string }) {
  const router = useRouter();
  const [, startTransition] = useTransition();
  const [current, setCurrent] = useState<Locale>(locale);

  function set(next: Locale) {
    if (next === current) return;
    setCurrent(next);
    document.cookie = `${LOCALE_COOKIE}=${next}; path=/; max-age=31536000; samesite=lax`;
    startTransition(() => router.refresh());
  }

  const btn = (l: Locale, text: string) => (
    <button
      type="button"
      aria-pressed={current === l}
      onClick={() => set(l)}
      className={cn(
        'rounded-full px-2 py-1 text-xs font-medium transition-colors',
        current === l ? 'bg-ink text-cloud' : 'text-muted hover:text-ink',
      )}
    >
      {text}
    </button>
  );

  return (
    <div
      role="group"
      aria-label={label}
      className="flex items-center gap-0.5 rounded-full border border-border p-0.5"
    >
      {btn('en', 'EN')}
      {btn('ja', '日本語')}
    </div>
  );
}
