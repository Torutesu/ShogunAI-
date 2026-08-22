import type { Metadata } from 'next';
import { LegalPage } from '@/components/LegalPage';
import { getI18n } from '@/i18n/server';

export const dynamic = 'force-dynamic';

export const metadata: Metadata = {
  title: 'Privacy Policy',
  description: 'How ShogunAI handles your data.',
  alternates: { canonical: '/privacy' },
};

export default async function PrivacyPage() {
  const { locale, t } = await getI18n();

  const content =
    locale === 'ja'
      ? {
          updated: '最終更新：2026年7月25日',
          intro:
            'ShogunAI は、あなたの仕事の文脈をローカルファーストで扱うことを前提に設計されています。このプライバシーポリシーは、ShogunAI のウェブサイト、待機リスト、関連するプロダクト体験で扱う情報、利用目的、保護方法を説明します。',
          sections: [
            {
              h: '1. 収集する情報',
              p: 'ウェブサイト上では、アーリーアクセスの案内に必要なメールアドレス、分析用の基本イベント、そして不正利用防止に必要な技術情報を収集することがあります。ShogunAI のプロダクト内で扱う記憶データや作業コンテキストは、既定ではあなたのデバイス上に保存され、当社サーバーへ自動送信される前提ではありません。',
            },
            {
              h: '2. 情報の利用目的',
              p: '収集した情報は、アーリーアクセスの案内、スパムや不正利用の防止、サービス品質の改善、重要なお知らせの送付のために利用します。あなたの明示的な許可なく、待機リスト登録情報を第三者広告のために販売することはありません。',
            },
            {
              h: '3. ローカルファーストの考え方',
              p: 'ShogunAI の中心的な価値は、あなたの文脈をあなたの管理下に置くことです。私たちは、記憶データ・検索対象・作業履歴について、可能な限りデバイス上で処理・保持される設計を優先します。クラウド同期や外部モデル連携を将来的に提供する場合も、共有範囲が分かる形で提示し、利用者が選べる前提で進めます。',
            },
            {
              h: '4. 共有と外部提供',
              p: '法令上の要請、権利保護、不正対策、インフラ運用に合理的に必要な場合を除き、個人情報を第三者へ提供しません。決済、分析、メール配信などの委託先を利用する場合は、必要最小限の情報に限定し、当社の指示に従って取り扱わせます。',
            },
            {
              h: '5. 保管期間と削除',
              p: '待機リストや問い合わせで取得した情報は、関係法令上または運営上必要な期間のみ保持します。不要となった情報は、合理的な期間内に削除または匿名化します。プロダクト内でローカル保存されたデータについては、利用者自身が削除またはアンインストールにより管理できる設計を目指しています。',
            },
            {
              h: '6. あなたの選択肢',
              p: 'メール配信は配信停止リンクから解除できます。待機リスト情報の削除、保有情報に関する問い合わせ、その他プライバシー関連の要望は info@shogunaios.com までご連絡ください。地域によっては、アクセス、訂正、削除、異議申立て等の法的権利を持つ場合があります。',
            },
            {
              h: '7. お問い合わせ',
              p: 'このポリシーや ShogunAI のプライバシー対応についてのご質問は、info@shogunaios.com までお送りください。',
            },
          ],
        }
      : locale === 'es'
        ? {
            updated: 'Última actualización: 25 de julio de 2026',
            intro: 'ShogunAI adopta un enfoque local-first para el contexto de trabajo. Esta Política de Privacidad explica qué información podemos recopilar a través del sitio, la lista de espera y el producto, por qué la utilizamos y cómo la protegemos.',
            sections: [
              { h: '1. Información que recopilamos', p: 'En el sitio podemos recopilar tu correo para avisos de acceso anticipado, eventos básicos de analítica e información técnica necesaria para prevenir abusos. En el producto, la memoria y el contexto de trabajo permanecen en tu dispositivo por defecto y no se envían automáticamente a nuestros servidores.' },
              { h: '2. Cómo utilizamos la información', p: 'Utilizamos la información para gestionar el acceso anticipado, prevenir spam y fraude, mejorar el servicio y enviar avisos importantes. No vendemos los datos de la lista de espera para publicidad de terceros sin tu permiso explícito.' },
              { h: '3. Nuestro enfoque local-first', p: 'Tu contexto debe permanecer bajo tu control. Priorizamos el procesamiento y almacenamiento en el dispositivo. Si se habilitan sincronización en la nube o proveedores externos, mostraremos qué se comparte y te dejaremos elegir.' },
              { h: '4. Proveedores y divulgación', p: 'No divulgamos información personal salvo cuando sea razonablemente necesario para cumplir la ley, proteger derechos, prevenir abusos u operar infraestructura esencial. Los proveedores reciben solo la información mínima necesaria.' },
              { h: '5. Conservación y eliminación', p: 'Conservamos los datos de la lista de espera y consultas solo durante el tiempo razonablemente necesario. La información que deja de ser necesaria se elimina o anonimiza. Los datos locales del producto pueden eliminarse mediante controles del producto o desinstalando el software.' },
              { h: '6. Tus opciones', p: 'Puedes darte de baja desde los correos recibidos. Para solicitar eliminación, acceso u otra petición de privacidad, escribe a info@shogunaios.com. Según tu lugar de residencia, puedes tener derechos adicionales.' },
              { h: '7. Contacto', p: 'Envía tus preguntas sobre esta política o nuestras prácticas de privacidad a info@shogunaios.com.' },
            ],
          }
        : locale === 'de'
          ? {
              updated: 'Zuletzt aktualisiert: 25. Juli 2026',
              intro: 'ShogunAI behandelt Arbeitskontext nach einem local-first Ansatz. Diese Datenschutzerklärung beschreibt, welche Informationen wir über Website, Warteliste und Produkt erfassen können, wofür wir sie verwenden und wie wir sie schützen.',
              sections: [
                { h: '1. Erfasste Informationen', p: 'Auf der Website können wir deine E-Mail-Adresse für Early-Access-Updates, grundlegende Analyseereignisse und technische Daten zur Missbrauchsprävention erfassen. Im Produkt bleiben Gedächtnis und Arbeitskontext standardmäßig auf deinem Gerät und werden nicht automatisch an unsere Server gesendet.' },
                { h: '2. Verwendung der Informationen', p: 'Wir verwenden Daten für Early Access, Spam- und Betrugsprävention, Qualitätsverbesserungen und wichtige Mitteilungen. Wartelistendaten werden ohne ausdrückliche Erlaubnis nicht für Werbung Dritter verkauft.' },
                { h: '3. Unser local-first Ansatz', p: 'Dein Kontext soll unter deiner Kontrolle bleiben. Wir priorisieren Verarbeitung und Speicherung auf dem Gerät. Bei optionaler Cloud-Synchronisierung oder externen Anbietern zeigen wir transparent, was geteilt wird, und überlassen dir die Wahl.' },
                { h: '4. Weitergabe und Dienstleister', p: 'Wir geben personenbezogene Daten nur weiter, wenn dies für gesetzliche Pflichten, den Schutz von Rechten, Missbrauchsprävention oder wesentliche Infrastruktur erforderlich ist. Dienstleister erhalten nur die minimal nötigen Informationen.' },
                { h: '5. Aufbewahrung und Löschung', p: 'Daten aus Warteliste und Anfragen werden nur so lange gespeichert, wie es betrieblich, rechtlich oder sicherheitsbedingt nötig ist. Nicht mehr benötigte Daten werden gelöscht oder anonymisiert. Lokale Produktdaten können über Produktkontrollen oder durch Deinstallation entfernt werden.' },
                { h: '6. Deine Wahlmöglichkeiten', p: 'Produkt-E-Mails können über den Abmeldelink beendet werden. Für Löschung, Auskunft oder andere Datenschutzanfragen erreichst du uns unter info@shogunaios.com. Je nach Wohnort können weitere gesetzliche Rechte gelten.' },
                { h: '7. Kontakt', p: 'Fragen zu dieser Erklärung oder unseren Datenschutzpraktiken sendest du an info@shogunaios.com.' },
              ],
            }
          : {
          updated: 'Last updated July 25, 2026',
          intro:
            'ShogunAI is built around a local-first view of work context. This Privacy Policy explains what information we may collect through the ShogunAI website, waitlist, and related product experiences, why we use it, and how we protect it.',
          sections: [
            {
              h: '1. Information we collect',
              p: 'On the website, we may collect your email address for early-access updates, basic product analytics events, and technical information needed to prevent abuse. Inside the ShogunAI product, memory data and work context are intended to stay on your device by default and are not assumed to be automatically sent to our servers.',
            },
            {
              h: '2. How we use information',
              p: 'We use collected information to manage early access, prevent spam and fraud, improve service quality, and send important product updates. We do not sell waitlist registration data for third-party advertising without your explicit permission.',
            },
            {
              h: '3. Our local-first approach',
              p: 'A core principle of ShogunAI is that your context should remain under your control. We prioritize designs where memory data, searchable context, and work history are processed and stored on-device wherever possible. If cloud sync or external model integrations are offered in the future, we aim to present them clearly and let you choose what is shared.',
            },
            {
              h: '4. Sharing and service providers',
              p: 'We do not disclose personal information to third parties except where reasonably necessary to comply with law, protect rights, prevent abuse, or operate core infrastructure. When we use vendors for services such as payments, analytics, or email delivery, we aim to limit them to the minimum information needed to perform those services on our behalf.',
            },
            {
              h: '5. Retention and deletion',
              p: 'We keep waitlist and inquiry data only for as long as reasonably necessary for operational, legal, and security purposes. When information is no longer needed, we aim to delete or anonymize it within a reasonable timeframe. For locally stored product data, our goal is to let you remove it through in-product controls or by uninstalling the software.',
            },
            {
              h: '6. Your choices',
              p: 'You can unsubscribe from product emails using the link in those messages. To request deletion of waitlist information, ask about the information we hold, or make another privacy-related request, contact info@shogunaios.com. Depending on where you live, you may also have legal rights to access, correct, delete, or object to certain uses of your information.',
            },
            {
              h: '7. Contact',
              p: 'Questions about this Privacy Policy or ShogunAI’s privacy practices can be sent to info@shogunaios.com.',
            },
          ],
        };

  return <LegalPage t={t} locale={locale} title={t.legalPage.privacyTitle} updated={content.updated} intro={content.intro} sections={content.sections} />;
}
