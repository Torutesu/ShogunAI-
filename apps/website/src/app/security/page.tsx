import type { Metadata } from 'next';
import { Check, KeyRound, Laptop, ShieldCheck } from 'lucide-react';
import { CTA } from '@/components/sections/CTA';
import { PageHeader, PageShell } from '@/components/PageShell';
import { JsonLd, breadcrumbSchema } from '@/components/seo/JsonLd';
import { Card } from '@/components/ui/card';
import { getI18n } from '@/i18n/server';
import { isLocale } from '@/i18n/config';
import { localizedAlternates, siteConfig } from '@/lib/site';

export const metadata: Metadata = {
  title: 'Privacy & security — Local-first AI memory',
  description: 'Learn how ShogunAI keeps work memory local-first, supports BYOK AI providers, and uses approval gates for consequential actions.',
  alternates: { canonical: '/en/security', languages: localizedAlternates('/security') },
};

const icons = [Laptop, KeyRound, ShieldCheck];
const content = {
  en: { eyebrow: 'Privacy & security', title: 'Your work context should remain yours', sub: 'ShogunAI is built around local-first memory, explicit provider choices, and human approval for consequential actions.', principles: [{ title: 'Local-first memory', body: 'Your captured work memory is designed to remain on your Mac by default, reducing the need for a permanent cloud copy of your day.' }, { title: 'Bring your own AI keys', body: 'Choose a supported provider and manage that relationship directly. Relevant context is shared only when an enabled request requires it.' }, { title: 'Approval before consequence', body: 'Actions that send, publish, modify, or otherwise matter are designed to pause for your review.' }], model: 'Control model', modelTitle: 'Know what stays local and what leaves', modelBody: 'Local-first does not mean every optional AI request happens entirely offline. When you ask a connected AI provider to process a request, the relevant information may be sent to that provider under its terms. ShogunAI makes that provider choice explicit through BYOK.', checks: ['Capture and searchable memory stay local by default', 'Connected tools are optional and permissioned', 'Your chosen AI provider processes approved requests', 'Consequential execution uses review gates', 'You can pause capture and remove local memory'], policies: 'Read the full policies', policiesBody: 'The product overview explains the design. The legal documents govern how the service is operated.', privacy: 'Privacy policy', terms: 'Terms of service' },
  ja: { eyebrow: 'プライバシー・セキュリティ', title: '仕事の文脈は、あなたのものであり続けるべきです', sub: 'ShogunAIは、ローカルファーストな記憶、明示的なプロバイダ選択、重要操作への人間の承認を中心に設計されています。', principles: [{ title: 'ローカルファーストな記憶', body: '取得した仕事の記憶は既定でMac内に保存され、一日の行動を恒久的にクラウドへ複製する必要を減らします。' }, { title: '自分のAI鍵を利用', body: '対応プロバイダを自分で選び、契約と鍵を直接管理します。必要な文脈は、有効にした処理で必要な場合だけ共有されます。' }, { title: '重要操作は実行前に承認', body: '送信、公開、変更など影響のある操作は、利用者が内容を確認してから進む設計です。' }], model: '管理モデル', modelTitle: '何が端末内に残り、何が外部へ送られるかを把握する', modelBody: 'ローカルファーストは、任意のAI処理まですべてオフラインという意味ではありません。連携したAIプロバイダへ処理を依頼すると、必要な情報がそのプロバイダの規約に基づいて送られる場合があります。ShogunAIはBYOKによって、その選択を明示します。', checks: ['取得した検索可能な記憶は既定で端末内に保存', 'ツール連携は任意で、明示的な権限が必要', '承認した依頼は選択したAIプロバイダが処理', '重要な実行には確認ステップを設定', '取得の一時停止とローカル記憶の削除が可能'], policies: '詳細なポリシーを確認', policiesBody: 'このページは設計上の概要です。実際のサービス運営にはプライバシーポリシーと利用規約が適用されます。', privacy: 'プライバシーポリシー', terms: '利用規約' },
  es: { eyebrow: 'Privacidad y seguridad', title: 'El contexto de tu trabajo debe seguir siendo tuyo', sub: 'ShogunAI se basa en memoria local-first, elección explícita de proveedor y aprobación humana para acciones importantes.', principles: [{ title: 'Memoria local-first', body: 'La memoria capturada permanece en tu Mac por defecto y reduce la necesidad de una copia permanente en la nube.' }, { title: 'Tus propias claves de IA', body: 'Elige un proveedor compatible y gestiona esa relación directamente.' }, { title: 'Aprobación antes de actuar', body: 'Los envíos, publicaciones y cambios importantes se detienen para tu revisión.' }], model: 'Modelo de control', modelTitle: 'Sabe qué permanece local y qué sale', modelBody: 'Local-first no significa que toda petición opcional de IA sea offline. Un proveedor conectado puede recibir la información necesaria bajo sus términos. BYOK hace explícita esa elección.', checks: ['La memoria consultable permanece local por defecto', 'Las conexiones son opcionales y autorizadas', 'Tu proveedor procesa solicitudes aprobadas', 'Las acciones importantes usan controles de revisión', 'Puedes pausar y eliminar memoria local'], policies: 'Consulta las políticas completas', policiesBody: 'Esta página explica el diseño. Los documentos legales rigen el servicio.', privacy: 'Política de privacidad', terms: 'Términos de servicio' },
  de: { eyebrow: 'Datenschutz & Sicherheit', title: 'Dein Arbeitskontext sollte dir gehören', sub: 'ShogunAI basiert auf local-first Gedächtnis, expliziter Anbieterwahl und menschlicher Freigabe wichtiger Aktionen.', principles: [{ title: 'Local-first Gedächtnis', body: 'Erfasste Arbeitserinnerung bleibt standardmäßig auf deinem Mac und reduziert dauerhafte Cloud-Kopien.' }, { title: 'Eigene KI-Schlüssel', body: 'Wähle einen unterstützten Anbieter und verwalte diese Beziehung direkt.' }, { title: 'Freigabe vor Folgen', body: 'Senden, Veröffentlichen und wichtige Änderungen pausieren zur Prüfung.' }], model: 'Kontrollmodell', modelTitle: 'Wisse, was lokal bleibt und was übertragen wird', modelBody: 'Local-first bedeutet nicht, dass jede optionale KI-Anfrage vollständig offline läuft. Ein verbundener Anbieter kann nötige Informationen unter seinen Bedingungen erhalten. BYOK macht diese Wahl ausdrücklich.', checks: ['Durchsuchbares Gedächtnis bleibt standardmäßig lokal', 'Verbindungen sind optional und berechtigt', 'Dein Anbieter verarbeitet freigegebene Anfragen', 'Wichtige Aktionen nutzen Prüfungen', 'Erfassung und lokale Erinnerung können entfernt werden'], policies: 'Vollständige Richtlinien lesen', policiesBody: 'Diese Seite erklärt das Design. Die rechtlichen Dokumente regeln den Dienst.', privacy: 'Datenschutzerklärung', terms: 'Nutzungsbedingungen' },
} as const;

export default async function SecurityPage({ searchParams }: { searchParams: Promise<{ _locale?: string }> }) {
  const requested = (await searchParams)._locale;
  const localeOverride = isLocale(requested) ? requested : undefined;
  const { locale, t } = await getI18n(localeOverride);
  const c = content[locale];
  const prefix = `/${locale}`;
  return (
    <PageShell locale={locale}>
      <JsonLd data={breadcrumbSchema([{ name: 'Home', url: `${siteConfig.url}${prefix}` }, { name: c.eyebrow, url: `${siteConfig.url}${prefix}/security` }])} />
      <PageHeader eyebrow={c.eyebrow} title={c.title} sub={c.sub} />
      <section className="py-[clamp(48px,7vw,88px)]">
        <div className="container-x grid gap-6 md:grid-cols-3">
          {c.principles.map(({ title, body }, index) => {
            const Icon = icons[index];
            return (
            <Card key={title} className="rounded-[26px] p-7"><span className="flex size-11 items-center justify-center rounded-xl bg-sky-soft text-accent"><Icon className="size-5" /></span><h2 className="mt-5 font-display text-xl font-semibold">{title}</h2><p className="mt-3 text-[15px] leading-relaxed text-muted">{body}</p></Card>
            );
          })}
        </div>
      </section>
      <section className="border-y border-border bg-cloud/45 py-[clamp(48px,7vw,88px)]">
        <div className="container-x grid gap-10 lg:grid-cols-2">
          <div><p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{c.model}</p><h2 className="mt-3 font-display text-[clamp(26px,4vw,40px)] font-semibold">{c.modelTitle}</h2><p className="mt-4 text-[16px] leading-relaxed text-muted">{c.modelBody}</p></div>
          <ul className="grid gap-3">{c.checks.map((item) => <li key={item} className="flex items-start gap-3 rounded-xl border border-border bg-surface p-4 text-sm"><Check className="mt-0.5 size-4 shrink-0 text-accent" />{item}</li>)}</ul>
        </div>
      </section>
      <section className="py-14"><div className="container-x max-w-[820px] text-center"><h2 className="font-display text-2xl font-semibold">{c.policies}</h2><p className="mt-3 text-muted">{c.policiesBody}</p><div className="mt-5 flex justify-center gap-3"><a className="text-sm font-semibold text-accent" href={`${prefix}/privacy`}>{c.privacy}</a><span className="text-border">·</span><a className="text-sm font-semibold text-accent" href={`${prefix}/terms`}>{c.terms}</a></div></div></section>
      <CTA t={t} />
    </PageShell>
  );
}
