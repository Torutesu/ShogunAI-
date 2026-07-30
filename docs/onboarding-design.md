# オンボーディング設計 — ダウンロードから「最初の本物の答え」まで

- 文書ID: `docs/onboarding-design.md`
- ステータス: 設計確定・実装済み（PR #90 の内容を現 main へ再構築。差分と統合判断は `docs/fixes/2026-07-30-onboarding-rebuild-design.md` が正）
- 上位文書: `CLAUDE.md`（絶対不変条件・SLO・プラン構成）、`docs/requirements-v1.0.md`
- 関連: GitHub issue #6（本設計の起点）、issue #46（AX許可ガイド。本フローに吸収）、`apps/desktop/src/onboarding/`

> **再構築時の変更（2026-07-30）**: 原案（PR #90）の §3.1「ノッチパネル内で走らせる」は、main で実機検証済みの専用オンボーディングウィンドウ（#46/#76 の `onboarding.html`）ホストに変更した。draft-stop の保存先は main 既存の `ComposioPolicy`（`composio.json`）に一本化した。パネル内ホストへの回帰は Phase 0 の Go 判定後の別 issue。

---

## 1. なぜオンボーディングが機能の付属物ではないか

SHOGUN は初回起動の時点で**中身が空**である。記録ツールなら「空でも使える」が、状態の推定と実行のプロダクトは、推定する材料がないうちは何も返せない。つまり初回のユーザーは、

- **見返りをまだ一度も受け取っていない状態で**、
- **常駐して画面を読む権限**という、このプロダクトで最も重い許可を求められる。

この非対称を埋めるのがオンボーディングの唯一の仕事である。したがって設計目標は「機能を説明すること」ではなく、**権限を渡すに足る根拠を、権限を求める前に渡すこと**。

### 設計原則

1. **順序が主張である。** 何をするか → 何を読み、何を残さないか → 権限 → プラン → 接続 → 呼び出し方。理由を渡す前に何も要求しない
2. **主張ではなく証拠を返す。** 権限付与の直後に「いま読んでいるアプリ名」をその場で表示する。動いていることは、書くのではなく見せる
3. **設定させない。設定不要であることを見せる。** 除外（パスワードマネージャ等）はビルドの事実であって設定項目ではない（§3.2）
4. **どのステップも中断・再開できる。** 進捗は Rust が持ち、途中で終了しても同じ場所から再開する
5. **スキップできるのは、スキップしても製品が壊れないステップだけ。** 説明だけのステップに「スキップ」は出さない

---

## 2. ダウンロードから最初の答えまで（アプリ外を含む全7ビート）

| # | ビート | 場所 | 設計上の判断 |
|---|---|---|---|
| 1 | ダウンロード | サイト | 配布は Developer ID + notarization（App Store 不使用）。DMG の中身は **アプリ本体とApplicationsのエイリアスの2つだけ**。README や説明ファイルを同梱しない — 読ませたい説明はすべてアプリ内にある |
| 2 | DMG を開く | Finder | 背景画像は「アプリ → Applications」の矢印のみ。装飾コピーを置かない |
| 3 | 初回起動 | macOS | notarization 済みなので Gatekeeper は「インターネットからダウンロードされました」の確認1回のみ。**ここで独自のダイアログを重ねない** |
| 4 | ウィンドウ出現 | SHOGUN | 起動後、専用オンボーディングウィンドウが前面に出る（Accessory app でも全 Space に浮く #46 実証済みレシピ） |
| 5 | 6ステップ | SHOGUN | §3 |
| 6 | 完了 | SHOGUN | 完了の書き込みで Rust がトライアルを刻印しウィンドウを閉じる。パネルは通常どおり |
| 7 | 最初の答え | SHOGUN | 空スレッドの提案を1クリック。**追跡済みの状態と、いま画面にあるものだけから作る**（捏造しない） |

### 計測する時間

「ダウンロード完了 → 最初の本物の答え」までの経過時間。ファネルイベントは Rust 側 allowlist を通り #91 の PostHog アダプタ（opt-out 尊重）で送る。**内容は運ばない**（step id のみ。CLAUDE.md）。

---

## 3. フロー本体（6ステップ）

実装: `apps/desktop/src/onboarding/Onboarding.tsx`。文言はすべて `src/strings.ts`（`ob*` 接頭辞 + 権限ステップは #46 の `onboarding` ブロックを再利用）。

### 3.1 ホスト面（再構築時の決定）

原案はノッチから降りるパネル内だったが、main では #46 の専用ウィンドウ（`onboarding.html`、720×640、全 Space フロート、`SHOGUN_FORCE_ONBOARDING=1` QA ハッチ）をホストにする。理由と回帰条件は `docs/fixes/2026-07-30-onboarding-rebuild-design.md` §4.1。

### 3.2 各ステップ

| # | id | 見出し | 目的 | スキップ |
|---|---|---|---|---|
| 1 | `welcome` | SHOGUN lives in the notch. | 一言定義と居場所 | 不可（説明のみ） |
| 2 | `reads` | What it reads, and what it never keeps. | プライバシー契約 + 読まないものの提示 | 不可（説明のみ） |
| 3 | `permission` | SHOGUN needs one permission. | Accessibility 付与と、その場での証拠（#46 ガイドを内包） | 可（付与済みなら非表示） |
| 4 | `plan` | Seven days of everything. | トライアル、Standard/Pro、キーの分離、（Pro のみ）BYOK 入力 | 可 |
| 5 | `connect` | Connect what you work in. | Wave 1 接続 + ドラフト止まりモード | 可 |
| 6 | `ready` | You're set. | 呼び出し方（実バインドから表示）と今夜の予告、計測トグル | — |

**ステップ2の設計判断（重要）**: 除外（パスワードマネージャ・認証ダイアログ・ターミナル・プライベートブラウジング）は**削除不能な既定**であり、設定項目ではない。ステップ2は「除外を設定させる」のではなく「**すでに守られている事実を見せる**」。カテゴリと件数は `exclusion_categories` で生きたポリシー（`ExclusionPolicy::category_counts`）から取る — UIが昨日の除外リストをハードコードすれば、今日の挙動について嘘をつくことになる。

**ステップ3の設計判断**: 付与の確認は**プロンプトを出さない** `accessibility_status` を1.5秒間隔でポーリング（+ Rust watcher の `accessibility-changed` push）。プロンプトを出すのはボタンを押した1回だけ（`open_accessibility_settings` = 一度きりのシステムプロンプト + Accessibility ペインの deep link）。付与された瞬間にカードが緑に変わり、いま読んでいるアプリ名を出す。

**ステップ4の設計判断**: 課金フロー本体（Stripe）はここに入れない。ここで決まるのは「どちらのプランを使うつもりか」だけで、**キーを訊くかどうかの分岐にしか使わない**。カード情報はトライアル終了まで求めない。キーは Keychain のみ（不変条件7）。

**ステップ5の設計判断**: 「ドラフト止まり」トグルは既定 ON で接続リストの**前**に置く。保存先は `ComposioPolicy`（送信ゲートと同じ唯一のレコード）。consent 未取得のまま OFF にする試みは Rust が拒否し、UI は ON に戻す（不変条件4のフェイルセーフ）。未実装サービスはこのステップでは出さない（Settings では出す）。

---

## 4. 権限を拒否したまま進んだ場合

Accessibility なしでも壊れない。ステップ3で「Without it」として明示する:

- 動く: 接続（読み取り）、チャット、設定、Nightly review の一部
- 動かない: いま何をしているかの把握、そこから出る提案、文脈のあるドラフト

Settings から再度オンボーディングを開く導線（`set_onboarding_state` で `completed: false` に戻す + ウィンドウ再表示）は follow-up（再構築設計 doc §5 Phase 3）。

---

## 5. Rust 側（実装済み）

契約は `apps/desktop/src/onboarding/ipc.ts` に1か所で定義。

| コマンド | 返り値 | 実装の置き場所 | 備考 |
|---|---|---|---|
| `onboarding_state` | `{ completed, step, plan, trial_started_at? }` | `onboarding.rs`（`app_data/onboarding.json`、version 付き JSON。純粋ロジックは `onboarding::state` で Linux でもテスト） | #46 の旧 disposition `{completed, skipped}` は completed へ移行（トライアルは捏造しない） |
| `set_onboarding_state` | — | 同上 | 全レコード書き込み（書き手が1つ）。`completed` が false→true になった最初の書き込みで `trial_started_at` を刻む。以後は再走しても刻み直さない。完了時にウィンドウを閉じる |
| `accessibility_status` | `bool` | `onboarding.rs` → `axcache::ax_trusted_silent()` | 非プロンプト版（ポーリング用） |
| `open_accessibility_settings` | — | `onboarding.rs` | 一度きりのシステムプロンプト＋ Accessibility ペインを `open` で開く |
| `exclusion_categories` | `[{ id, count }]` | `exclusions.rs` + `shogun-core/capture/exclusion.rs` `category_counts()` | 生きたポリシーから数える。未設置時は空にフェイルクローズ。id: `password_managers` / `auth_dialog` / `terminals` / `private_browsing` / `sensitive_titles` |
| （draft-stop） | — | `approvals.rs` の `composio_settings` / `set_composio_policy` を再利用 | **既定 ON**。consent なしの OFF は拒否 → UI は ON に戻す（不変条件4）。オンボーディング専用の保存先は持たない |
| `onboarding_event` | — | `onboarding.rs` → `analytics.rs`（#91） | イベント名は Rust 側 allowlist。opt-out 尊重。内容は運ばない |

加えて:

- **MCP/CLI 対称性（不変条件6）— 契約を実装、実データ配線は follow-up**: Memory API に `Tool::DeviceOnboardingGet`（wire `device.onboarding.get`、Read）を追加し、MCP `tools/list`・REST `GET /v1/device/onboarding`・CLI `shogun onboarding` の三面で露出。ただしオンボーディング状態は desktop の app-settings にあり、`DbBackend`（core DB）はこれを持たないため、当面この面は空を返す（捏造しない）。実データ供給は共有ストア化の別 issue
- **プラン判定は Rust 側**（CLAUDE.md）。ステップ4の選択は意思表明であって、機能ゲートの根拠にしてはならない。**実際の entitlement enforcement はまだ存在しない**（課金実装時の follow-up issue）

---

## 6. 受け入れ基準

- [ ] ダウンロード完了 → 最初の本物の答え までの中央値を計測できる（目標: **10分以内**、うちアプリ内は3分以内）
- [ ] 各ステップがスキップ可能（§3.2 の可否表どおり）で、中断しても同じ場所から再開する
- [ ] Accessibility 未付与のままでもアプリが壊れず、できないことが UI に明示される
- [ ] オンボーディング完了状態が MCP/CLI からも取得できる（契約は済み、実データは follow-up）
- [ ] 全文言が `strings.ts` 経由で、コンポーネントに直書きされていない
- [ ] 除外カテゴリの表示が、生きた `ExclusionPolicy` と一致する（ハードコードしない）
- [ ] キーは Keychain 以外に書かれない（平文ファイル・DB・ログ禁止）

## 7. まだ決めていないこと

- ~~トライアルの起点~~ **決定: オンボーディング完了時点**。`set_onboarding_state` が完了を永続化する最初の書き込みで `trial_started_at` を刻む。途中離脱者のトライアルが始まらない件（離脱検知）は別 issue
- 接続 0 件のまま完了させるか（現状は許可。「最初の答え」が画面だけを根拠にすることになる）
- 2台目以降のデバイスでのオンボーディング（同期は v2 スコープなので、v1 は毎回フル）
- #46 旧 disposition から移行した既存デバイスのトライアル起点（再構築設計 doc §6）
