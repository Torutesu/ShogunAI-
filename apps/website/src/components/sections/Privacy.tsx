import Link from 'next/link';
import { ChevronRight, Database, Fingerprint, Lock, ShieldCheck } from 'lucide-react';
import { Reveal } from '@/components/animations/Reveal';
import type { Dictionary } from '@/i18n/dictionaries';
import type { Locale } from '@/i18n/config';

const ICONS = [Fingerprint, ShieldCheck, Database];
const TONES = [
  'from-[#8faaff] via-[#dbe3ff] to-[#f5f7ff]',
  'from-[#fff1ba] via-[#fff8df] to-[#fff6c8]',
  'from-[#9bb7ff] via-[#dce7ff] to-[#f2f5ff]',
] as const;

const CHIPS = ['On-device capture', 'No cloud copy by default', 'BYOK enabled'] as const;

type PrivacyCopy = {
  chips: readonly string[];
  capturedLocally: string;
  captureDescription: string;
  privateByDefault: string;
  noSilentSync: string;
  encrypted: string;
  auditable: string;
  modelAccess: string;
  providerLimits: string;
  capturePolicy: string;
  remembered: string;
  screenMemory: string;
  allowed: string;
  privateApps: string;
  ignored: string;
  deleteForever: string;
  oneTap: string;
  securityDefaults: string;
  securityItems: readonly string[];
};

function PrivacyVisual({ index, copy }: { index: number; copy: PrivacyCopy }) {
  if (index === 0) {
    return (
      <div className="relative h-[240px] overflow-hidden rounded-[24px] bg-[radial-gradient(circle_at_50%_18%,rgba(255,255,255,0.8),transparent_30%),linear-gradient(180deg,#9bb4ff_0%,#dce5ff_100%)] p-5">
        <div className="absolute left-5 top-5 flex size-14 items-center justify-center rounded-[19px] bg-white/60 text-[#1744bd] shadow-[inset_0_1px_0_rgba(255,255,255,0.65)] backdrop-blur">
          <Fingerprint className="size-7" strokeWidth={1.8} />
        </div>
        <div className="absolute bottom-5 left-5 right-5 rounded-[24px] border border-white/65 bg-white/48 p-4 backdrop-blur">
          <div className="text-sm font-semibold text-[#17316f]">{copy.capturedLocally}</div>
          <div className="mt-1 text-[13px] leading-relaxed text-[#36518c]">{copy.captureDescription}</div>
        </div>
      </div>
    );
  }

  if (index === 1) {
    return (
      <div className="relative h-[240px] overflow-hidden rounded-[24px] bg-[radial-gradient(circle_at_50%_18%,rgba(255,255,255,0.8),transparent_30%),linear-gradient(180deg,#fff0b7_0%,#fff7d8_100%)] p-5">
        <div className="absolute left-5 top-5 flex size-14 items-center justify-center rounded-[19px] bg-white/60 text-[#8d6618] shadow-[inset_0_1px_0_rgba(255,255,255,0.65)] backdrop-blur">
          <ShieldCheck className="size-7" strokeWidth={1.8} />
        </div>
        <div className="absolute inset-x-5 bottom-5 grid grid-cols-2 gap-2.5">
          {[copy.privateByDefault, copy.noSilentSync, copy.encrypted, copy.auditable].map((chip) => (
            <div key={chip} className="flex min-h-11 items-center justify-center rounded-[15px] border border-[#2b2b2b]/12 bg-white/68 px-2.5 py-2 text-center text-[11px] font-semibold leading-tight text-[#3e3a2e] backdrop-blur sm:text-[12px]">
              {chip}
            </div>
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="relative h-[240px] overflow-hidden rounded-[24px] bg-[radial-gradient(circle_at_50%_18%,rgba(255,255,255,0.8),transparent_30%),linear-gradient(180deg,#abc4ff_0%,#dce8ff_100%)] p-5">
      <div className="absolute left-5 top-5 flex size-14 items-center justify-center rounded-[19px] bg-white/60 text-[#2a50bc] shadow-[inset_0_1px_0_rgba(255,255,255,0.65)] backdrop-blur">
        <Database className="size-7" strokeWidth={1.8} />
      </div>
      <div className="absolute bottom-6 right-6 rounded-[26px] border border-white/60 bg-white/42 px-5 py-4 text-right backdrop-blur">
        <div className="text-[12px] font-semibold uppercase tracking-[0.08em] text-[#536a9e]">{copy.modelAccess}</div>
        <div className="mt-2 text-[28px] font-semibold tracking-[-0.03em] text-[#1d326e]">BYOK</div>
        <div className="mt-1 text-[13px] text-[#6176a3]">{copy.providerLimits}</div>
      </div>
    </div>
  );
}

export function Privacy({ t, locale }: { t: Dictionary; locale: Locale }) {
  const copies: Record<Locale, PrivacyCopy> = {
    ja: {
          chips: ['端末内で取得', '既定でクラウド複製なし', 'BYOK 対応'],
          capturedLocally: '端末内で取得',
          captureDescription: '明示的に許可しない限り、端末の外には出ません。',
          privateByDefault: '既定でプライベート',
          noSilentSync: '自動同期なし',
          encrypted: '暗号化済み',
          auditable: '監査可能',
          modelAccess: 'モデルアクセス',
          providerLimits: 'プロバイダも上限も、自分で選ぶ。',
          capturePolicy: '取得ポリシー',
          remembered: '何を記憶するかは、あなたが決める。',
          screenMemory: '画面のメモリ',
          allowed: '許可',
          privateApps: 'プライベートアプリ',
          ignored: '除外',
          deleteForever: '完全に削除',
          oneTap: 'ワンタップ',
          securityDefaults: 'セキュリティの既定値',
          securityItems: ['暗号化されたローカル保存', 'データの学習利用なし', '共有は必要な範囲だけ', 'BYOK 対応'],
        },
    es: {
          chips: ['Captura en el dispositivo', 'Sin copia en la nube por defecto', 'Compatible con BYOK'],
          capturedLocally: 'Capturado localmente',
          captureDescription: 'Nada sale de tu equipo sin tu permiso explícito.',
          privateByDefault: 'Privado por defecto',
          noSilentSync: 'Sin sincronización oculta',
          encrypted: 'Cifrado',
          auditable: 'Auditable',
          modelAccess: 'Acceso al modelo',
          providerLimits: 'Tu proveedor. Tus límites.',
          capturePolicy: 'Política de captura',
          remembered: 'Tú decides qué se recuerda.',
          screenMemory: 'Memoria de pantalla',
          allowed: 'Permitida',
          privateApps: 'Apps privadas',
          ignored: 'Excluidas',
          deleteForever: 'Eliminar para siempre',
          oneTap: 'Un toque',
          securityDefaults: 'Seguridad por defecto',
          securityItems: ['Almacenamiento local cifrado', 'Tus datos no se usan para entrenamiento', 'Solo se comparte lo necesario', 'Compatible con BYOK'],
        },
    de: {
          chips: ['Erfassung auf dem Gerät', 'Standardmäßig keine Cloud-Kopie', 'BYOK-Unterstützung'],
          capturedLocally: 'Lokal erfasst',
          captureDescription: 'Ohne deine ausdrückliche Freigabe verlässt nichts dein Gerät.',
          privateByDefault: 'Standardmäßig privat',
          noSilentSync: 'Keine stille Synchronisierung',
          encrypted: 'Verschlüsselt',
          auditable: 'Nachvollziehbar',
          modelAccess: 'Modellzugriff',
          providerLimits: 'Dein Anbieter. Deine Grenzen.',
          capturePolicy: 'Erfassungsregeln',
          remembered: 'Du bestimmst, was erinnert wird.',
          screenMemory: 'Bildschirmgedächtnis',
          allowed: 'Erlaubt',
          privateApps: 'Private Apps',
          ignored: 'Ausgeschlossen',
          deleteForever: 'Dauerhaft löschen',
          oneTap: 'Ein Tippen',
          securityDefaults: 'Sichere Voreinstellungen',
          securityItems: ['Verschlüsselter lokaler Speicher', 'Kein Training mit deinen Daten', 'Nur erforderlicher Kontext wird geteilt', 'BYOK-Unterstützung'],
        },
    en: {
          chips: CHIPS,
          capturedLocally: 'Captured locally',
          captureDescription: 'Nothing leaves your machine unless you explicitly allow it.',
          privateByDefault: 'Private by default',
          noSilentSync: 'No silent sync',
          encrypted: 'Encrypted',
          auditable: 'Auditable',
          modelAccess: 'Model access',
          providerLimits: 'Your provider. Your limits.',
          capturePolicy: 'Capture policy',
          remembered: 'You decide what is remembered.',
          screenMemory: 'Screen memory',
          allowed: 'Allowed',
          privateApps: 'Private apps',
          ignored: 'Ignored',
          deleteForever: 'Delete forever',
          oneTap: 'One tap',
          securityDefaults: 'Security defaults',
          securityItems: ['Encrypted local store', 'No training on your data', 'Scoped sharing only', 'BYOK supported'],
        },
  };
  const copy = copies[locale];

  return (
    <section id="privacy" className="theme-soft-section scroll-mt-20 bg-[#fffdf5] py-[clamp(56px,9vw,112px)]">
      <div className="container-x">
        <div className="grid items-center gap-10 lg:grid-cols-[minmax(300px,0.78fr)_minmax(0,1.22fr)]">
          <Reveal>
            <div className="max-w-[30rem]">
              <p className="text-xs font-semibold uppercase tracking-[0.08em] text-accent">{t.privacy.eyebrow}</p>
              <h2 className="privacy-title mt-4 font-display text-[clamp(36px,4.8vw,62px)] font-semibold leading-[1.04] tracking-[-0.045em] text-balance">
                {t.privacy.title}
              </h2>
              <p className="privacy-body mt-5 max-w-[34ch] text-[18px] leading-relaxed text-muted">{t.privacy.body}</p>
              <Link
                href={`/${locale}/security`}
                className="group/pl mt-7 inline-flex items-center gap-1 text-sm font-semibold text-accent-strong transition-colors hover:text-accent"
              >
                {t.privacy.cta}
                <ChevronRight className="size-4 transition-transform duration-200 group-hover/pl:translate-x-0.5" />
              </Link>
            </div>
          </Reveal>

          <Reveal delay={0.06}>
            <div className="theme-light-panel overflow-hidden rounded-[34px] border border-white/60 bg-[linear-gradient(135deg,rgba(255,255,255,0.9),rgba(239,251,255,0.84))] p-5 shadow-[0_22px_70px_rgba(9,11,12,0.08)] backdrop-blur md:p-7">
              <div className="grid gap-3 sm:grid-cols-3">
                {copy.chips.map((chip) => (
                  <div key={chip} className="flex min-h-14 items-center justify-center rounded-[20px] border border-white/75 bg-white/72 px-3 py-2.5 text-center text-[12px] font-semibold leading-tight text-[#0d5c77] shadow-[0_10px_22px_rgba(17,40,68,0.04)] sm:text-[13px]">
                    {chip}
                  </div>
                ))}
              </div>

              <div className="mt-5 grid gap-4 xl:grid-cols-[minmax(0,0.94fr)_minmax(0,1.06fr)]">
                <div className="relative rounded-[28px] border border-white/70 bg-[#0f1620] p-5 text-white shadow-[0_16px_42px_rgba(8,13,18,0.18)]">
                  <div className="pr-14">
                    <div>
                      <div className="text-[12px] font-semibold uppercase tracking-[0.08em] text-white/55">{copy.capturePolicy}</div>
                      <div className="mt-2 text-[clamp(22px,2vw,26px)] font-semibold leading-[1.3] tracking-[-0.03em] text-balance">{copy.remembered}</div>
                    </div>
                  </div>
                  <div className="absolute right-5 top-5 flex size-11 items-center justify-center rounded-2xl bg-white/10">
                    <Lock className="size-5" strokeWidth={1.8} />
                  </div>

                  <div className="mt-5 space-y-3">
                    {[
                      [copy.screenMemory, copy.allowed],
                      [copy.privateApps, copy.ignored],
                      [copy.deleteForever, copy.oneTap],
                    ].map(([label, value]) => (
                      <div key={label} className="flex min-h-12 items-center justify-between gap-3 rounded-[18px] border border-white/10 bg-white/6 px-4 py-3">
                        <span className="min-w-0 text-[13px] leading-tight text-white/68">{label}</span>
                        <span className="rounded-full bg-white/10 px-2.5 py-1 text-[12px] font-semibold text-white">{value}</span>
                      </div>
                    ))}
                  </div>
                </div>

                <div className="theme-light-panel rounded-[28px] border border-[#dceef8] bg-white/80 p-5 shadow-[0_16px_42px_rgba(17,40,68,0.05)]">
                  <div className="text-[12px] font-semibold uppercase tracking-[0.08em] text-[#6d8793]">{copy.securityDefaults}</div>
                  <div className="mt-3 grid gap-3 sm:grid-cols-2">
                    {copy.securityItems.map((item) => (
                      <div key={item} className="privacy-security-item flex min-h-14 items-center rounded-[18px] border border-[#e7f2f8] bg-[#f8fdff] px-4 py-3 text-[13px] font-medium leading-snug text-[#294c59]">
                        {item}
                      </div>
                    ))}
                  </div>
                </div>
              </div>
            </div>
          </Reveal>
        </div>

        <div className="mt-8 grid gap-6 md:grid-cols-2 xl:grid-cols-3">
          {t.privacy.points.map((p, i) => {
            return (
              <Reveal key={p.title} delay={i * 0.08 + 0.08}>
                <div className="theme-light-panel lift overflow-hidden rounded-[30px] border border-border/70 bg-white/84 p-4 shadow-[0_18px_50px_rgba(9,11,12,0.06)]">
                  <div className="mb-6">
                    <PrivacyVisual index={i} copy={copy} />
                  </div>

                  <div className="px-2 pb-2 text-center">
                    <div className={`font-display text-[clamp(24px,2vw,32px)] font-semibold leading-[1.15] tracking-[-0.03em] text-ink ${locale === 'ja' ? 'break-keep' : 'text-balance'}`}>
                      {p.title}
                    </div>
                    <div className="privacy-point-body mx-auto mt-4 max-w-[24ch] text-[16px] leading-relaxed text-muted">{p.body}</div>
                  </div>
                </div>
              </Reveal>
            );
          })}
        </div>
      </div>
    </section>
  );
}
