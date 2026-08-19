import { ChevronDown, Menu } from 'lucide-react';
import { Logo } from '@/components/Logo';
import { LanguageMenu } from '@/components/LanguageMenu';
import { SoundInteractions } from '@/components/SoundInteractions';
import { SoundToggle } from '@/components/SoundToggle';
import { ThemeToggle } from '@/components/ThemeToggle';
import { Button } from '@/components/ui/button';
import { getI18n } from '@/i18n/server';
import type { Locale } from '@/i18n/config';

export async function Nav({ localeOverride }: { localeOverride?: Locale } = {}) {
  const { locale, t } = await getI18n(localeOverride);
  const prefix = localeOverride ? `/${localeOverride}` : '';
  const labels = {
    en: { overview: 'Overview', recall: 'Contextual recall', useCases: 'Use cases', useCasesOverview: 'All use cases', founders: 'For founders', product: 'For product & engineering', consultants: 'For consultants', integrations: 'Integrations', market: 'Market' },
    ja: { overview: '機能一覧', recall: '文脈検索', useCases: '活用事例', useCasesOverview: '活用事例一覧', founders: '創業者向け', product: 'プロダクト・開発向け', consultants: 'コンサルタント向け', integrations: '連携', market: '市場' },
    es: { overview: 'Resumen', recall: 'Recuperación contextual', useCases: 'Casos de uso', useCasesOverview: 'Todos los casos', founders: 'Para fundadores', product: 'Para producto e ingeniería', consultants: 'Para consultores', integrations: 'Integraciones', market: 'Mercado' },
    de: { overview: 'Übersicht', recall: 'Kontextsuche', useCases: 'Anwendungsfälle', useCasesOverview: 'Alle Anwendungsfälle', founders: 'Für Gründer', product: 'Für Produkt & Entwicklung', consultants: 'Für Berater', integrations: 'Integrationen', market: 'Markt' },
  }[locale];

  const features = [
    { href: `${prefix}/features`, label: labels.overview },
    { href: `${prefix}/features/ai-memory`, label: t.nav.memory },
    { href: `${prefix}/features/contextual-recall`, label: labels.recall },
    { href: `${prefix}/features/execution-layer`, label: t.nav.action },
  ];
  const useCaseLinks = [
    { href: `${prefix}/use-cases`, label: labels.useCasesOverview },
    { href: `${prefix}/use-cases/founders`, label: labels.founders },
    { href: `${prefix}/use-cases/product-engineering`, label: labels.product },
    { href: `${prefix}/use-cases/consultants`, label: labels.consultants },
  ];
  const primary = [
    { href: `${prefix}/integrations`, label: labels.integrations },
    { href: `${prefix}/pricing`, label: t.nav.pricing },
    { href: `${prefix}/market`, label: labels.market },
    { href: `${prefix}/blog`, label: t.nav.blog },
  ];
  const menuLabel = locale === 'ja' ? 'メニュー' : locale === 'es' ? 'Menú' : locale === 'de' ? 'Menü' : 'Menu';

  return (
    <header className="sticky top-0 z-50 border-b border-border/70 bg-surface/72 backdrop-blur-2xl backdrop-saturate-150">
      <div className="container-x flex h-16 items-center justify-between gap-2 sm:gap-4">
        <a href={`${prefix}/#top`} aria-label="ShogunAI home" className="group/brand flex items-center gap-2.5">
          <Logo size={26} className="brand-logo" />
          <span className="font-display text-lg font-semibold tracking-tight">ShogunAI</span>
        </a>

        <nav aria-label="Primary" className="hidden items-center gap-5 lg:flex xl:gap-7">
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

          {/* Use-cases dropdown */}
          <div className="group relative">
            <button
              type="button"
              className="flex items-center gap-1 text-sm font-medium text-muted transition-colors hover:text-ink group-focus-within:text-ink"
            >
              {labels.useCases}
              <ChevronDown className="size-3.5 transition-transform group-hover:rotate-180" />
            </button>
            <div className="invisible absolute left-1/2 top-full z-50 -translate-x-1/2 pt-3 opacity-0 transition-all duration-150 group-hover:visible group-hover:opacity-100 group-focus-within:visible group-focus-within:opacity-100">
              <div className="min-w-[220px] rounded-2xl border border-border bg-surface p-2 shadow-[var(--shadow-float)]">
                {useCaseLinks.map((l) => (
                  <a
                    key={l.href}
                    href={l.href}
                    className="block rounded-lg px-3 py-2 text-sm font-medium text-muted transition-colors hover:bg-cloud hover:text-ink"
                  >
                    {l.label}
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
          <SoundInteractions />
          <SoundToggle />
          <ThemeToggle />
          <LanguageMenu current={locale} label={t.nav.langLabel} />
          <Button asChild size="sm" className="hidden shadow-none sm:inline-flex">
            <a href={`${prefix}/#get-started`}>{t.nav.getStarted}</a>
          </Button>

          {/* Mobile menu — JS-free disclosure */}
          <details className="group relative lg:hidden">
            <summary aria-label={menuLabel} title={menuLabel} className="flex size-11 cursor-pointer list-none items-center justify-center rounded-full border border-border bg-surface/90 text-ink [&::-webkit-details-marker]:hidden">
              <Menu className="size-5" aria-hidden="true" />
            </summary>
            <div className="absolute right-0 top-11 w-52 rounded-xl border border-border bg-surface p-2 shadow-[var(--shadow-float)]">
              {[...features, ...useCaseLinks, ...primary].map((l) => (
                <a key={l.href} href={l.href} className="block rounded-lg px-3 py-2.5 text-sm font-medium text-ink hover:bg-cloud">
                  {l.label}
                </a>
              ))}
              <a
                href={`${prefix}/#get-started`}
                className="mt-1 block rounded-lg border border-[#f1d8a8]/60 bg-[linear-gradient(135deg,var(--cta-start),var(--cta-end))] px-3 py-3 text-center text-sm font-semibold text-[var(--cta-ink)] shadow-[0_2px_10px_rgba(115,78,20,0.22)]"
              >
                {t.nav.getStarted}
              </a>
            </div>
          </details>
        </div>
      </div>
    </header>
  );
}
