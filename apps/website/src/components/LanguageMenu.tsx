'use client';

import { Globe } from 'lucide-react';
import { usePathname, useRouter } from 'next/navigation';
import { useState, useTransition } from 'react';
import { LOCALE_COOKIE, type Locale, locales, localeNames } from '@/i18n/config';
import { cn } from '@/lib/utils';

/** Globe-icon language switcher. Keeps mobile selection independent from hover state. */
export function LanguageMenu({ current, label }: { current: Locale; label: string }) {
  const router = useRouter();
  const pathname = usePathname();
  const [, startTransition] = useTransition();
  const [active, setActive] = useState<Locale>(current);
  const [open, setOpen] = useState(false);

  function pick(l: Locale) {
    setActive(l);
    setOpen(false);
    document.cookie = `${LOCALE_COOKIE}=${l}; path=/; max-age=31536000; samesite=lax`;
    const segments = pathname.split('/').filter(Boolean);
    const localizedRoots = new Set(['features', 'use-cases', 'integrations', 'security', 'pricing', 'compare', 'blog']);
    const first = segments[0];
    let nextPath = pathname;
    if (first && (locales as readonly string[]).includes(first)) {
      nextPath = `/${[l, ...segments.slice(1)].join('/')}`;
    } else if (pathname === '/') {
      nextPath = `/${l}`;
    } else if (first && localizedRoots.has(first)) {
      nextPath = `/${l}${pathname}`;
    }
    startTransition(() => {
      if (nextPath === pathname) router.refresh();
      else router.push(nextPath);
    });
  }

  return (
    <div className="relative" onMouseEnter={() => setOpen(true)} onMouseLeave={() => setOpen(false)}>
      <button
        type="button"
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        className="group flex size-11 items-center justify-center rounded-full border border-border text-muted transition-colors hover:border-ink/25 hover:text-ink"
      >
        <Globe className="size-4 transition-transform duration-500 group-hover:rotate-[24deg]" />
      </button>
      <div className={`absolute right-0 top-full z-50 pt-2 transition-all duration-150 ${open ? 'visible translate-y-0 opacity-100' : 'invisible -translate-y-1 opacity-0'}`}>
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
