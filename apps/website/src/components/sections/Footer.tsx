import { Logo } from '@/components/Logo';
import { getI18n } from '@/i18n/server';

export async function Footer() {
  const { t } = await getI18n();
  const cols = [
    {
      title: t.footer.product.title,
      links: [
        { href: '/#memory', label: t.footer.product.memory },
        { href: '/#action', label: t.footer.product.action },
        { href: '/#pricing', label: t.footer.product.pricing },
      ],
    },
    {
      title: t.footer.company.title,
      links: [
        { href: '/about', label: t.footer.company.about },
        { href: '/blog', label: t.footer.company.blog },
        { href: '/careers', label: t.footer.company.careers },
      ],
    },
    {
      title: t.footer.legal.title,
      links: [
        { href: '/privacy', label: t.footer.legal.privacy },
        { href: '/terms', label: t.footer.legal.terms },
      ],
    },
  ];

  return (
    <footer className="border-t border-border pt-14">
      <div className="container-x grid gap-10 pb-10 md:grid-cols-[1.4fr_2fr]">
        <div>
          <a href="/#top" className="flex items-center gap-2.5">
            <Logo size={22} />
            <span className="font-display text-lg font-semibold tracking-tight">ShogunAI</span>
          </a>
          <p className="mt-3.5 text-xs text-muted">{t.footer.tagline}</p>
        </div>
        <div className="grid grid-cols-2 gap-6 sm:grid-cols-3">
          {cols.map((col) => (
            <div key={col.title} className="flex flex-col gap-3">
              <div className="text-xs font-medium uppercase tracking-[0.06em] text-muted">{col.title}</div>
              {col.links.map((l) => (
                <a key={l.label} href={l.href} className="text-sm text-muted transition-colors hover:text-ink">
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
