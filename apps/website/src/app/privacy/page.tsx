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
            'ShogunAI は、あなたの仕事の文脈をローカルファーストで扱うことを前提に設計されています。このプライバシーポリシーは、ShogunAI のウェブサイト、待機リスト、関連するプロダクト体験において、どの情報を収集し、なぜ使い、どのように保護するかを説明するドラフトです。',
          sections: [
            {
              h: '1. 収集する情報',
              p: 'ウェブサイト上では、メールアドレス、紹介コード、分析用の基本イベント、そして不正利用防止に必要な技術情報を収集することがあります。ShogunAI のプロダクト内で扱う記憶データや作業コンテキストは、既定ではあなたのデバイス上に保存され、当社サーバーへ自動送信される前提ではありません。',
            },
            {
              h: '2. 情報の利用目的',
              p: '収集した情報は、アーリーアクセスの案内、待機リスト順位や紹介状況の反映、スパムや不正利用の防止、サービス品質の改善、重要なお知らせの送付のために利用します。あなたの明示的な許可なく、待機リスト登録情報を第三者広告のために販売することはありません。',
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
              p: 'メール配信は配信停止リンクから解除できます。待機リスト情報の削除、保有情報に関する問い合わせ、その他プライバシー関連の要望は hello@shogunai.com までご連絡ください。地域によっては、アクセス、訂正、削除、異議申立て等の法的権利を持つ場合があります。',
            },
            {
              h: '7. お問い合わせ',
              p: 'このドラフトポリシーや ShogunAI のプライバシー対応についてのご質問は、hello@shogunai.com までお送りください。',
            },
          ],
        }
      : {
          updated: 'Last updated July 25, 2026',
          intro:
            'ShogunAI is built around a local-first view of work context. This Privacy Policy is a draft that explains what information we may collect through the ShogunAI website, waitlist, and related product experiences, why we use it, and how we aim to protect it.',
          sections: [
            {
              h: '1. Information we collect',
              p: 'On the website, we may collect your email address, referral code, basic product analytics events, and technical information needed to prevent abuse. Inside the ShogunAI product, memory data and work context are intended to stay on your device by default and are not assumed to be automatically sent to our servers.',
            },
            {
              h: '2. How we use information',
              p: 'We use collected information to manage early access, operate the waitlist and referral system, prevent spam and fraud, improve service quality, and send important product updates. We do not sell waitlist registration data for third-party advertising without your explicit permission.',
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
              p: 'You can unsubscribe from product emails using the link in those messages. To request deletion of waitlist information, ask about the information we hold, or make another privacy-related request, contact hello@shogunai.com. Depending on where you live, you may also have legal rights to access, correct, delete, or object to certain uses of your information.',
            },
            {
              h: '7. Contact',
              p: 'Questions about this draft Privacy Policy or ShogunAI’s privacy practices can be sent to hello@shogunai.com.',
            },
          ],
        };

  return <LegalPage t={t} title={t.legalPage.privacyTitle} updated={content.updated} intro={content.intro} sections={content.sections} />;
}
