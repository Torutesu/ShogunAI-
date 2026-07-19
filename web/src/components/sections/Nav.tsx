import { Menu } from 'lucide-react';
import { Logo } from '@/components/Logo';
import { LocaleToggle } from '@/components/LocaleToggle';
import { Button } from '@/components/ui/button';
import { getI18n } from '@/i18n/server';

export async function Nav() {
  const { locale, t } = await getI18n();
  const links = [
    { href: '/#memory', label: t.nav.memory },
    { href: '/#action', label: t.nav.action },
    { href: '/#how', label: t.nav.how },
    { href: '/#testimonials', label: t.nav.testimonials },
    { href: '/#pricing', label: t.nav.pricing },
  ];

  return (
    <header className="sticky top-0 z-50 border-b border-border bg-surface/70 backdrop-blur-xl backdrop-saturate-150">
      <div className="container-x flex h-16 items-center justify-between">
        <a href="/#top" aria-label="ShogunAI home" className="flex items-center gap-2.5">
          <Logo size={26} />
          <span className="font-display text-lg font-semibold tracking-tight">ShogunAI</span>
        </a>

        <nav aria-label="Primary" className="hidden items-center gap-7 md:flex">
          {links.map((l) => (
            <a key={l.href} href={l.href} className="text-sm font-medium text-muted transition-colors hover:text-ink">
              {l.label}
            </a>
          ))}
        </nav>

        <div className="flex items-center gap-2.5">
          <LocaleToggle locale={locale} label={t.nav.langLabel} />
          <Button asChild variant="secondary" size="sm" className="hidden sm:inline-flex">
            <a href="/#get-started">{t.nav.signIn}</a>
          </Button>
          <Button asChild size="sm" className="hidden sm:inline-flex">
            <a href="/#get-started">{t.nav.getStarted}</a>
          </Button>

          {/* Mobile menu — JS-free disclosure */}
          <details className="group relative md:hidden">
            <summary className="flex size-9 cursor-pointer list-none items-center justify-center rounded-full border border-border text-ink [&::-webkit-details-marker]:hidden">
              <Menu className="size-5" aria-label="Menu" />
            </summary>
            <div className="absolute right-0 top-11 w-52 rounded-xl border border-border bg-surface p-2 shadow-[var(--shadow-float)]">
              {links.map((l) => (
                <a key={l.href} href={l.href} className="block rounded-lg px-3 py-2 text-sm font-medium text-ink hover:bg-cloud">
                  {l.label}
                </a>
              ))}
              <a href="/#get-started" className="mt-1 block rounded-lg bg-ink px-3 py-2 text-center text-sm font-medium text-cloud">
                {t.nav.getStarted}
              </a>
            </div>
          </details>
        </div>
      </div>
    </header>
  );
}
