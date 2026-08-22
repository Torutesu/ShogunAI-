import { AnimatedLogo } from '@/components/AnimatedLogo';
import { getI18n } from '@/i18n/server';
import type { Locale } from '@/i18n/config';

export async function Footer({ localeOverride }: { localeOverride?: Locale } = {}) {
  const { locale, t } = await getI18n(localeOverride);
  const prefix = localeOverride ? `/${localeOverride}` : '';
  const labels = {
    en: { features: 'Features', memory: 'AI memory', recall: 'Contextual recall', execution: 'Execution layer', useCases: 'Use cases', founders: 'For founders', product: 'Product & engineering', consultants: 'For consultants', resources: 'Resources', integrations: 'Integrations', security: 'Privacy & security', market: 'Market' },
    ja: { features: '機能', memory: 'AIメモリ', recall: '文脈検索', execution: '実行レイヤー', useCases: '活用事例', founders: '創業者向け', product: 'プロダクト・開発向け', consultants: 'コンサルタント向け', resources: 'リソース', integrations: '連携', security: 'プライバシーと安全性', market: '市場' },
    es: { features: 'Funciones', memory: 'Memoria de IA', recall: 'Recuperación contextual', execution: 'Capa de ejecución', useCases: 'Casos de uso', founders: 'Para fundadores', product: 'Producto e ingeniería', consultants: 'Para consultores', resources: 'Recursos', integrations: 'Integraciones', security: 'Privacidad y seguridad', market: 'Mercado' },
    de: { features: 'Funktionen', memory: 'KI-Gedächtnis', recall: 'Kontextsuche', execution: 'Ausführungsebene', useCases: 'Anwendungsfälle', founders: 'Für Gründer', product: 'Produkt & Entwicklung', consultants: 'Für Berater', resources: 'Ressourcen', integrations: 'Integrationen', security: 'Datenschutz & Sicherheit', market: 'Markt' },
  }[locale];
  const cols = [
    {
      title: labels.features,
      links: [
        { href: `${prefix}/features/ai-memory`, label: labels.memory },
        { href: `${prefix}/features/contextual-recall`, label: labels.recall },
        { href: `${prefix}/features/execution-layer`, label: labels.execution },
        { href: `${prefix}/pricing`, label: t.footer.product.pricing },
      ],
    },
    { title: labels.useCases, links: [{ href: `${prefix}/use-cases/founders`, label: labels.founders }, { href: `${prefix}/use-cases/product-engineering`, label: labels.product }, { href: `${prefix}/use-cases/consultants`, label: labels.consultants }] },
    { title: labels.resources, links: [{ href: `${prefix}/integrations`, label: labels.integrations }, { href: `${prefix}/security`, label: labels.security }, { href: `${prefix}/market`, label: labels.market }, { href: `${prefix}/blog`, label: t.footer.company.blog }] },
    {
      title: t.footer.company.title,
      links: [
        { href: `${prefix}/about`, label: t.footer.company.about },
        { href: '/careers', label: t.footer.company.careers },
        { href: '/rules', label: t.campaign.cta },
      ],
    },
    {
      title: t.footer.legal.title,
      links: [
        { href: `${prefix}/privacy`, label: t.footer.legal.privacy },
        { href: `${prefix}/terms`, label: t.footer.legal.terms },
      ],
    },
  ];

  return (
    <footer className="border-t border-border pt-14">
      <div className="container-x grid gap-12 pb-10 lg:grid-cols-[minmax(240px,0.9fr)_minmax(0,3.1fr)] xl:gap-16">
        <div>
          <a href={`${prefix}/#top`} data-mark-hover className="flex items-center gap-2.5">
            <AnimatedLogo size={22} />
            <span className="font-display text-lg font-semibold tracking-tight">ShogunAI</span>
          </a>
          <p className="mt-3.5 text-xs text-muted">{t.footer.tagline}</p>
        </div>
        <div className="grid grid-cols-2 gap-x-10 gap-y-8 sm:grid-cols-3 lg:gap-x-7 xl:grid-cols-5 xl:gap-x-10">
          {cols.map((col) => (
            <div key={col.title} className="flex flex-col gap-3">
              <div className="text-xs font-medium uppercase tracking-[0.06em] text-muted hyphens-auto break-words">{col.title}</div>
              {col.links.map((l) => (
                <a key={l.label} href={l.href} className="text-sm text-muted transition-colors hover:text-ink hyphens-auto break-words">
                  {l.label}
                </a>
              ))}
            </div>
          ))}
        </div>
      </div>
      <div className="container-x flex flex-wrap items-center justify-between gap-3 border-t border-border py-[22px] pb-10">
        <span className="text-xs text-muted">{t.footer.rights}</span>
        <span className="text-xs text-muted">{t.footer.madeFor}</span>
      </div>
    </footer>
  );
}
