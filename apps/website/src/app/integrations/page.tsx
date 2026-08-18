import type { Metadata } from 'next';
import { ArrowRight } from 'lucide-react';
import { CTA } from '@/components/sections/CTA';
import { PageHeader, PageShell } from '@/components/PageShell';
import { JsonLd, breadcrumbSchema } from '@/components/seo/JsonLd';
import { Badge } from '@/components/ui/badge';
import { Card } from '@/components/ui/card';
import { BrandIcon } from '@/components/BrandIcon';
import { getI18n } from '@/i18n/server';
import { isLocale } from '@/i18n/config';
import { localizedAlternates, siteConfig } from '@/lib/site';

export const metadata: Metadata = {
  title: 'Integrations — Connect AI memory to your work tools',
  description: 'ShogunAI connects private work memory and execution across 50+ tools, including communication, documents, product, design, and AI providers.',
  alternates: { canonical: '/en/integrations', languages: localizedAlternates('/integrations') },
};

type IntegrationTool = {
  name: string;
  domain: string;
};

const COMMUNICATION_TOOLS: readonly IntegrationTool[] = [
  { name: 'Slack', domain: 'slack.com' },
  { name: 'Gmail', domain: 'gmail.com' },
  { name: 'Discord', domain: 'discord.com' },
  { name: 'Microsoft Teams', domain: 'teams.microsoft.com' },
  { name: 'Outlook', domain: 'outlook.com' },
  { name: 'Zoom', domain: 'zoom.us' },
  { name: 'Telegram', domain: 'telegram.org' },
  { name: 'WhatsApp', domain: 'whatsapp.com' },
  { name: 'Messenger', domain: 'messenger.com' },
  { name: 'X', domain: 'x.com' },
  { name: 'LinkedIn', domain: 'linkedin.com' },
  { name: 'Intercom', domain: 'intercom.com' },
];

const KNOWLEDGE_TOOLS: readonly IntegrationTool[] = [
  { name: 'Notion', domain: 'notion.so' },
  { name: 'Google Drive', domain: 'drive.google.com' },
  { name: 'Dropbox', domain: 'dropbox.com' },
  { name: 'OneDrive', domain: 'onedrive.live.com' },
  { name: 'Confluence', domain: 'confluence.com' },
  { name: 'Box', domain: 'box.com' },
  { name: 'Coda', domain: 'coda.io' },
  { name: 'Airtable', domain: 'airtable.com' },
  { name: 'Evernote', domain: 'evernote.com' },
  { name: 'Obsidian', domain: 'obsidian.md' },
  { name: 'Miro', domain: 'miro.com' },
  { name: 'ClickUp', domain: 'clickup.com' },
];

const PRODUCT_TOOLS: readonly IntegrationTool[] = [
  { name: 'Linear', domain: 'linear.app' },
  { name: 'GitHub', domain: 'github.com' },
  { name: 'Figma', domain: 'figma.com' },
  { name: 'Asana', domain: 'asana.com' },
  { name: 'Jira', domain: 'jira.com' },
  { name: 'Trello', domain: 'trello.com' },
  { name: 'monday.com', domain: 'monday.com' },
  { name: 'Framer', domain: 'framer.com' },
  { name: 'Webflow', domain: 'webflow.com' },
  { name: 'Canva', domain: 'canva.com' },
  { name: 'Sketch', domain: 'sketch.com' },
  { name: 'Adobe', domain: 'adobe.com' },
  { name: 'GitLab', domain: 'gitlab.com' },
  { name: 'Vercel', domain: 'vercel.com' },
];

const AI_TOOLS: readonly IntegrationTool[] = [
  { name: 'OpenAI', domain: 'openai.com' },
  { name: 'Anthropic', domain: 'anthropic.com' },
  { name: 'Perplexity', domain: 'perplexity.ai' },
  { name: 'Gemini', domain: 'gemini.google.com' },
  { name: 'Microsoft Copilot', domain: 'copilot.microsoft.com' },
  { name: 'xAI', domain: 'x.ai' },
  { name: 'Mistral AI', domain: 'mistral.ai' },
  { name: 'Cohere', domain: 'cohere.com' },
  { name: 'DeepSeek', domain: 'deepseek.com' },
  { name: 'Together AI', domain: 'together.ai' },
  { name: 'Groq', domain: 'groq.com' },
  { name: 'OpenRouter', domain: 'openrouter.ai' },
  { name: 'Hugging Face', domain: 'huggingface.co' },
  { name: 'Replicate', domain: 'replicate.com' },
];

const content = {
  en: { eyebrow: 'Integrations', title: 'Your context layer across the tools you already use', sub: 'ShogunAI is designed to connect memory and action across 50+ tools. Connections are optional, controlled by you, and do not require moving your work into a new system.', groups: [{ title: 'Communication', tools: COMMUNICATION_TOOLS, body: 'Bring conversations and follow-ups into the context surrounding your work.' }, { title: 'Knowledge & documents', tools: KNOWLEDGE_TOOLS, body: 'Connect documents and working knowledge without replacing where your team already writes.' }, { title: 'Product & design', tools: PRODUCT_TOOLS, body: 'Carry decisions from discussion through design, implementation, and project updates.' }, { title: 'AI providers', tools: AI_TOOLS, body: 'Use supported providers with your own keys and choose where approved context is processed.' }], noteTitle: 'Integrations deepen context. They are not required.', note: 'ShogunAI’s local-first memory is the foundation. Connecting tools can add richer context and enable approved actions, while your provider keys and permissions remain under your control.', link: 'How privacy works' },
  ja: { eyebrow: '連携', title: '使い慣れたツールを横断する、あなたの文脈レイヤー', sub: 'ShogunAIは50以上のツールをまたいで、記憶と実行をつなぐ設計です。連携は任意で、権限は自分で管理でき、仕事を新しいシステムへ移す必要はありません。', groups: [{ title: 'コミュニケーション', tools: COMMUNICATION_TOOLS, body: '会話やフォローアップを、仕事全体の文脈と関連付けます。' }, { title: 'ナレッジ・文書', tools: KNOWLEDGE_TOOLS, body: 'チームが現在利用している文書環境を置き換えず、資料と業務知識をつなぎます。' }, { title: 'プロダクト・デザイン', tools: PRODUCT_TOOLS, body: '議論から設計、実装、進捗報告まで、意思決定の文脈を保ちます。' }, { title: 'AIプロバイダ', tools: AI_TOOLS, body: '自分の鍵で対応プロバイダを利用し、承認した文脈をどこで処理するか選べます。' }], noteTitle: '連携は文脈を深めますが、必須ではありません。', note: '基盤になるのは、ShogunAIのローカルファーストな記憶です。ツール連携によって文脈と実行機能を拡張しつつ、鍵と権限は自分で管理できます。', link: 'プライバシーの仕組み' },
  es: { eyebrow: 'Integraciones', title: 'Tu capa de contexto entre las herramientas que ya utilizas', sub: 'ShogunAI conecta memoria y acción entre más de 50 herramientas. Las conexiones son opcionales y no exigen migrar tu trabajo.', groups: [{ title: 'Comunicación', tools: COMMUNICATION_TOOLS, body: 'Relaciona conversaciones y seguimientos con el contexto de tu trabajo.' }, { title: 'Conocimiento y documentos', tools: KNOWLEDGE_TOOLS, body: 'Conecta documentos sin sustituir el lugar donde tu equipo escribe.' }, { title: 'Producto y diseño', tools: PRODUCT_TOOLS, body: 'Conserva decisiones desde la discusión hasta la implementación.' }, { title: 'Proveedores de IA', tools: AI_TOOLS, body: 'Usa tus propias claves y elige dónde se procesa el contexto aprobado.' }], noteTitle: 'Las integraciones amplían el contexto, pero no son obligatorias.', note: 'La memoria local-first es la base. Las conexiones añaden contexto y acciones aprobadas, mientras tus claves y permisos siguen bajo tu control.', link: 'Cómo funciona la privacidad' },
  de: { eyebrow: 'Integrationen', title: 'Deine Kontextebene über die Tools, die du bereits nutzt', sub: 'ShogunAI verbindet Erinnerung und Handlung über mehr als 50 Tools. Verbindungen sind optional und erfordern keinen Workflow-Wechsel.', groups: [{ title: 'Kommunikation', tools: COMMUNICATION_TOOLS, body: 'Verbinde Gespräche und Follow-ups mit deinem Arbeitskontext.' }, { title: 'Wissen & Dokumente', tools: KNOWLEDGE_TOOLS, body: 'Verbinde Dokumente, ohne den Arbeitsort deines Teams zu ersetzen.' }, { title: 'Produkt & Design', tools: PRODUCT_TOOLS, body: 'Bewahre Entscheidungen von der Diskussion bis zur Umsetzung.' }, { title: 'KI-Anbieter', tools: AI_TOOLS, body: 'Nutze eigene Schlüssel und wähle, wo freigegebener Kontext verarbeitet wird.' }], noteTitle: 'Integrationen vertiefen Kontext, sind aber nicht erforderlich.', note: 'Das local-first Gedächtnis ist die Basis. Verbindungen ergänzen Kontext und freigegebene Aktionen, während Schlüssel und Berechtigungen unter deiner Kontrolle bleiben.', link: 'So funktioniert Datenschutz' },
} as const;

export default async function IntegrationsPage({ searchParams }: { searchParams: Promise<{ _locale?: string }> }) {
  const requested = (await searchParams)._locale;
  const localeOverride = isLocale(requested) ? requested : undefined;
  const { locale, t } = await getI18n(localeOverride);
  const c = content[locale];
  const prefix = `/${locale}`;
  const collectionSchema = { '@context': 'https://schema.org', '@type': 'CollectionPage', name: `ShogunAI ${c.eyebrow}`, description: c.sub, url: `${siteConfig.url}${prefix}/integrations`, mainEntity: c.groups.flatMap((group) => group.tools).map((tool) => ({ '@type': 'SoftwareApplication', name: tool.name })) };
  return (
    <PageShell locale={locale}>
      <JsonLd data={collectionSchema as Record<string, unknown>} />
      <JsonLd data={breadcrumbSchema([{ name: 'Home', url: `${siteConfig.url}${prefix}` }, { name: c.eyebrow, url: `${siteConfig.url}${prefix}/integrations` }])} />
      <PageHeader eyebrow={c.eyebrow} title={c.title} sub={c.sub} />
      <section className="py-[clamp(48px,7vw,88px)]">
        <div className="container-x grid gap-6 md:grid-cols-2">
          {c.groups.map((group) => (
            <Card key={group.title} className="rounded-[26px] p-7">
              <h2 className="font-display text-2xl font-semibold">{group.title}</h2>
              <p className="mt-3 text-[15px] leading-relaxed text-muted">{group.body}</p>
              <div className="mt-6 flex flex-wrap gap-2.5">
                {group.tools.map((tool) => (
                  <Badge key={tool.domain} className="gap-2 border border-border bg-cloud py-2 pl-2 pr-3 text-[13px]">
                    <BrandIcon domain={tool.domain} name={tool.name} size={20} className="size-5 rounded-[5px]" />
                    {tool.name}
                  </Badge>
                ))}
              </div>
            </Card>
          ))}
        </div>
        <div className="mx-auto mt-12 max-w-[820px] rounded-[26px] border border-border bg-cloud/55 p-7 text-center">
          <h2 className="font-display text-2xl font-semibold">{c.noteTitle}</h2>
          <p className="mx-auto mt-3 max-w-[62ch] text-[15px] leading-relaxed text-muted">{c.note}</p>
          <a href={`${prefix}/security`} className="mt-5 inline-flex items-center gap-2 text-sm font-semibold text-accent">{c.link} <ArrowRight className="size-4" /></a>
        </div>
      </section>
      <CTA t={t} />
    </PageShell>
  );
}
