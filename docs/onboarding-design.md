# オンボーディング設計 — ダウンロードから「最初の本物の答え」まで

- 文書ID: `docs/onboarding-design.md`
- ステータス: 設計確定・実装済み（フロントエンド＋Rust。§5 の全コマンドが `src-tauri` に入り、実機で初回オンボーディングが表示される。MCP/CLI 対称性は§5参照）
- 上位文書: `CLAUDE.md`（絶対不変条件・SLO・プラン構成）、`docs/requirements-v1.0.md`
- 関連: GitHub issue #6（本設計の起点）、`apps/desktop/src/onboarding/`

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
| 3 | 初回起動 | macOS | notarization 済みなので Gatekeeper は「インターネットからダウンロードされました」の確認1回のみ。**ここで独自のダイアログを重ねない**（OSのダイアログの上に自前の説明を出すと、どちらがOSのものか分からなくなる） |
| 4 | パネル出現 | SHOGUN | 起動後、ノッチから **オンボーディングがパネルとして降りてくる**。中央のウィンドウではない（§3.1） |
| 5 | 6ステップ | SHOGUN | §3 |
| 6 | 完了 | SHOGUN | パネルが通常サイズに戻り、チャットが開く |
| 7 | 最初の答え | SHOGUN | 空スレッドの提案（"What should I do first?" 等）を1クリック。**追跡済みの状態と、いま画面にあるものだけから作る**（捏造しない） |

### 計測する時間

「ダウンロード完了 → 最初の本物の答え」までの経過時間を、**端末内のみ**で計測する（キャプチャ内容はログに含めない、CLAUDE.md）。目標は §6。

---

## 3. フロー本体（6ステップ）

実装: `apps/desktop/src/onboarding/Onboarding.tsx`。文言はすべて `src/strings.ts`（`ob*` 接頭辞）。

### 3.1 なぜパネル内で走らせるか

中央モーダルではなく、**ノッチから降りるパネルそのもの**でオンボーディングを行う。理由:

- このプロダクトについて最初に覚えるべき事実は「どこに住んでいるか」である。最初の画面がその場所に出れば、説明が1つ減る
- 2ステップ目以降は、そのままパネルへ目を向ける動作の反復になる
- 既存の `set_panel_size` でサイズを変えるだけで済む（新規ウィンドウ・新規 NSPanel を作らない）

サイズは 660×600pt 固定（`W_ONBOARD` / `H_ONBOARD`）。ステップごとに高さを変えない — パネルが跳ねると、読む対象ではなく動く対象になる。

### 3.2 各ステップ

| # | id | 見出し | 目的 | スキップ |
|---|---|---|---|---|
| 1 | `welcome` | SHOGUN lives in the notch. | 一言定義と居場所 | 不可（説明のみ） |
| 2 | `reads` | What it reads, and what it never keeps. | プライバシー契約 + 読まないものの提示 | 不可（説明のみ） |
| 3 | `permission` | SHOGUN needs one permission. | Accessibility 付与と、その場での証拠 | 可（付与済みなら非表示） |
| 4 | `plan` | Seven days of everything. | トライアル、Standard/Pro、キーの分離、（Pro のみ）BYOK 入力 | 可 |
| 5 | `connect` | Connect what you work in. | Wave 1 接続 + ドラフト止まりモード | 可 |
| 6 | `ready` | You're set. | 呼び出し方（⌃⌥N / ⌥タップ）と今夜の予告 | — |

**ステップ2の設計判断（重要）**: 当初は「読まないアプリをユーザーに選ばせる」ステップを想定していたが、`apps/desktop/src-tauri/src/exclusions.rs` に既存の判断があった —

> **No settings UI, deliberately.** per-app on/off switches asked the user to curate a list of bundle identifiers to answer a question the product should answer itself.

これは正しい。パスワードマネージャ・認証ダイアログ・ターミナル・プライベートブラウジングは**削除不能な既定**であり、ユーザーが設定して初めて守られるものではない。したがってステップ2は「除外を設定させる」のではなく「**すでに守られている事実を見せる**」。カテゴリと件数は `exclusion_categories` で生きたポリシーから取る — UIが昨日の除外リストをハードコードすれば、今日の挙動について嘘をつくことになる。

**ステップ3の設計判断**: 付与の確認は**プロンプトを出さない**方の API を1.5秒間隔でポーリングする。`AXIsProcessTrustedWithOptions` にプロンプトオプションを付けたままポーリングすると、システムダイアログが1.5秒ごとに開き直る。プロンプトを出すのはボタンを押した1回だけ（`request_ax_permission`）。付与された瞬間にカードが緑に変わり、「Right now: Mail」と、いま読んでいるアプリ名を返す。

**ステップ4の設計判断**: 課金フロー本体（Stripe）はここに入れない。ここで決まるのは「どちらのプランを使うつもりか」だけで、それは**キーを訊くかどうかの分岐にしか使わない**。カード情報はトライアル終了まで求めない。

**ステップ5の設計判断**: 「ドラフト止まり」は既定 ON でトグルを**接続リストの前**に置く。接続してから送信ポリシーを説明するのでは順序が逆になる。未実装サービス（Notion/GitHub/Linear）は**このステップでは出さない** — 終わらせようとしている画面に、できないことを並べない（Settings では出す。ロードマップは見えていてよい）。

---

## 4. 権限を拒否したまま進んだ場合

Accessibility なしでも壊れない。ステップ3で「Without it」として明示する:

- 動く: 接続（読み取り）、チャット、設定、Nightly review の一部
- 動かない: いま何をしているかの把握、そこから出る提案、文脈のあるドラフト

Settings から再度オンボーディングを開けるようにする（Rust 側の `set_onboarding_state` で `completed: false` に戻すだけ）。トークン失効・権限剥奪からの復帰導線は、同じ部品（`perm` カード / 接続行の琥珀の矢印）を再利用する。

---

## 5. Rust 側（実装済み）

以下のコマンドが `src-tauri` に入り、実機で初回オンボーディングが表示される（`onboarding_state` が無い旧ビルドでは `getOnboardingState` が「完了済み」を返すため常にスキップされていた。契約は `apps/desktop/src/onboarding/ipc.ts` に1か所で定義）。すべて TDD（RED→GREEN）で実装。

| コマンド | 返り値 | 実装の置き場所 | 備考 |
|---|---|---|---|
| `onboarding_state` | `{ completed, step, plan, trial_started_at? }` | `lib.rs` `mod onboarding`（`app_data/onboarding.json`、`mod shortcuts` と同じ version 付き JSON 方式） | 年単位で生きる値ではないので DB ではなくアプリ設定 |
| `set_onboarding_state` | — | 同上 | 全レコード書き込み（書き手が1つ）。`completed` が false→true になった最初の書き込みで `trial_started_at` を刻む（§7 決定）。以後は再走しても刻み直さない |
| `ax_permission` | `bool` | `axcache.rs` `ax_trusted_silent()` | プロンプトオプションを `false` にした非プロンプト版 |
| `request_ax_permission` | — | `axcache.rs` | 一度きりのシステムプロンプト＋ System Settings の Accessibility ペインを `open` で開く |
| `exclusion_categories` | `[{ id, count }]` | `exclusions.rs`（プロセス全体ポリシー）＋ `shogun-core/capture/exclusion.rs` `category_counts()` | 生きたポリシーの const 配列長から数える。未設置時は空にフェイルクローズ。id は `password_managers` / `auth_dialog` / `terminals` / `private_browsing` / `sensitive_titles` |
| `get_draft_stop` / `set_draft_stop` | `bool` | `connectors.rs`（`app_data/draft-stop`、起動時に `build_runtime` を seed／`set` はライブランタイムにも反映） | **既定 ON**。欠落・空・破損はすべて ON にフェイルセーフ（不変条件4） |

加えて:

- **MCP/CLI 対称性（不変条件6）— 契約を実装、実データ配線は follow-up**: Memory API に `Tool::DeviceOnboardingGet`（wire `device.onboarding.get`、Read）を追加し、MCP `tools/list`・REST `GET /v1/device/onboarding`・CLI `shogun onboarding` の三面で露出（全面ユニットテスト済み）。ただしオンボーディング状態は desktop の app-settings（`onboarding.json`）にあり、実データを供給する `DbBackend`（core DB）はこれを持たないため、当面この面は空を返す（捏造しない）。実データを供給するには状態を共有ストア（core DB 等）に移す必要があり、これは別 issue とする
- **プラン判定は Rust 側**（CLAUDE.md）。ステップ4の選択は意思表明であって、機能ゲートの根拠にしてはならない

---

## 6. 受け入れ基準

- [ ] ダウンロード完了 → 最初の本物の答え までの中央値を端末内で計測できる（目標: **10分以内**、うちアプリ内は3分以内）
- [ ] 各ステップがスキップ可能（§3.2 の可否表どおり）で、中断しても同じ場所から再開する
- [ ] Accessibility 未付与のままでもアプリが壊れず、できないことが UI に明示される
- [ ] オンボーディング完了状態が MCP/CLI からも取得できる
- [ ] 全文言が `strings.ts` 経由で、コンポーネントに直書きされていない
- [ ] 除外カテゴリの表示が、生きた `ExclusionPolicy` と一致する（ハードコードしない）
- [ ] キーは Keychain 以外に書かれない（平文ファイル・DB・ログ禁止）

## 7. まだ決めていないこと

- ~~トライアルの起点~~ **決定: オンボーディング完了時点**（issue #6 実装時）。`set_onboarding_state` が完了を永続化する最初の書き込みで `trial_started_at` を刻む。途中離脱者のトライアルが始まらない件（離脱検知）は別 issue
- 接続 0 件のまま完了させるか（現状は許可。「最初の答え」が画面だけを根拠にすることになる）
- 2台目以降のデバイスでのオンボーディング（同期は v2 スコープなので、v1 は毎回フル）
