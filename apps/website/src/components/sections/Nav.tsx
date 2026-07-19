import { ChevronDown, Menu } from 'lucide-react';
import { Logo } from '@/components/Logo';
import { LanguageMenu } from '@/components/LanguageMenu';
import { ThemeToggle } from '@/components/ThemeToggle';
import { Button } from '@/components/ui/button';
import { getI18n } from '@/i18n/server';

export async function Nav() {
  const { locale, t } = await getI18n();

  const features = [
    { href: '/#memory', label: t.nav.memory },
    { href: '/#action', label: t.nav.action },
    { href: '/#testimonials', label: t.nav.testimonials },
  ];
  const primary = [
    { href: '/#how', label: t.nav.how },
    { href: '/#pricing', label: t.nav.pricing },
    { href: '/blog', label: t.nav.blog },
  ];

  return (
    <header className="sticky top-0 z-50 border-b border-border bg-surface/70 backdrop-blur-xl backdrop-saturate-150">
      <div className="container-x flex h-16 items-center justify-between">
        <a href="/#top" aria-label="ShogunAI home" className="group/brand flex items-center gap-2.5">
          <Logo size={26} className="brand-logo" />
          <span className="font-display text-lg font-semibold tracking-tight">ShogunAI</span>
        </a>

        <nav aria-label="Primary" className="hidden items-center gap-7 md:flex">
          {/* Features dropdown */}
          <div className="group relative">
            <button
              type="button"
              className="flex items-center gap-1 text-sm font-medium text-muted transition-colors hover:text-ink group-focus-within:text-ink"
            >
              {t.nav.features}
              <ChevronDown className="size-3.5 transition-transform group-hover:rotate-180" />
            </button>
            <div className="invisible absolute left-1/2 top-full z-50 -translate-x-1/2 pt-3 opacity-0 transition-all duration-150 group-hover:visible group-hover:opacity-100 group-focus-within:visible group-focus-within:opacity-100">
              <div className="min-w-[180px] rounded-2xl border border-border bg-surface p-2 shadow-[var(--shadow-float)]">
                {features.map((f) => (
                  <a
                    key={f.href}
                    href={f.href}
                    className="block rounded-lg px-3 py-2 text-sm font-medium text-muted transition-colors hover:bg-cloud hover:text-ink"
                  >
                    {f.label}
                  </a>
                ))}
              </div>
            </div>
          </div>

          {primary.map((l) => (
            <a
              key={l.href}
              href={l.href}
              className="relative text-sm font-medium text-muted transition-colors after:absolute after:-bottom-1.5 after:left-0 after:h-px after:w-0 after:bg-ink after:transition-all after:duration-300 after:content-[''] hover:text-ink hover:after:w-full"
            >
              {l.label}
            </a>
          ))}
        </nav>

        <div className="flex items-center gap-2.5">
          <ThemeToggle />
          <LanguageMenu current={locale} label={t.nav.langLabel} />
          <Button asChild size="sm" className="hidden sm:inline-flex">
            <a href="/#get-started">{t.nav.getStarted}</a>
          </Button>

          {/* Mobile menu — JS-free disclosure */}
          <details className="group relative md:hidden">
            <summary className="flex size-9 cursor-pointer list-none items-center justify-center rounded-full border border-border text-ink [&::-webkit-details-marker]:hidden">
              <Menu className="size-5" aria-label="Menu" />
            </summary>
            <div className="absolute right-0 top-11 w-52 rounded-xl border border-border bg-surface p-2 shadow-[var(--shadow-float)]">
              {[...features, ...primary].map((l) => (
                <a key={l.href} href={l.href} className="block rounded-lg px-3 py-2 text-sm font-medium text-ink hover:bg-cloud">
                  {l.label}
                </a>
              ))}
              <a href="/#get-started" className="mt-1 block rounded-lg bg-ink px-3 py-2 text-center text-sm font-medium text-on-ink">
                {t.nav.getStarted}
              </a>
            </div>
          </details>
        </div>
      </div>
    </header>
  );
}
