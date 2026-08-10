# Stripe 決済フロー本体（issue #8）設計記録

日付: 2026-08-10
関連: docs/requirements-v1.0.md §6.12（FR-BIL-01〜09）/ CLAUDE.md「プラン構成」/ docs/fixes/2026-07-31-entitlement-enforcement-design.md §5（Stripe seam）/ Issue #97（エンタイトルメント強制）

issue #97 で「プラン権利の強制」は入ったが、`BillingState` は常に `Unknown` のスタブだった（＝購入経路が存在しない）。本コミットでその2箇所のスタブを実物に置き換え、**決済 → Webhook → 自社DB → ライセンス → デバイス側プラン解決**を一本に繋いだ。

---

## 0. 金額の最新化（issue 本文との差分）

issue #8 の本文は「月額 $50–60 相当」と書かれているが、これは **2026-07-26 のプライシング決定（Free廃止・全員課金）より前の記述**。実装・LP・カタログはすべて現行価格に揃えた。

| プラン | 年払い | 月払い | トライアル |
|---|---|---|---|
| Standard | **$588/年（= $49/月）** | **$62/月** | 7日フル（Pro相当） |
| Pro | **$1,188/年（= $99/月）** | **$124/月** | 7日フル（Pro相当） |

- 通貨は USD（OPEN-05「価格ローカライズ」は未決のまま。v1 は USD 建て）
- 単一の真実は `apps/website/src/lib/pricing.ts`。LP コピー（`src/i18n/dictionaries.ts`, 4ロケール）と一致していることを `tests/billing.test.ts` が検査する（年額 = 月額換算 ×12 も検査）
- 価格変更の手順: `pricing.ts` → `dictionaries.ts` → Stripe ダッシュボード（Price は新規作成。既存 Price は編集不可）

---

## 1. フロー全体

```
[LP /#pricing]  or  [App 設定 → Plan & billing]
        │  plan 名 + interval のみ（Price ID はクライアントに出さない）
        ▼
POST /api/stripe/checkout ──► Stripe Checkout（ホスト型・カードUIは自前実装しない）
        │                                   │
        │ success_url                       │ webhook
        ▼                                   ▼
/billing/success?session_id=…      POST /api/stripe/webhook（署名検証必須・冪等）
   ライセンスキーを1度だけ提示            │ subscriptions を upsert / licenses を発行
                                          ▼
                                    自社DB（Postgres）
                                          ▲
        App（Keychain のライセンスキー）  │ POST /api/license/verify
        └──────────────────────────────────┘  { license_key, device_id, app_version }
                    ◄── Ed25519 署名付きライセンストークン
                          │
                          ▼
                  billing.json（署名付きトークンのキャッシュ）
                          │
   shogun_license::verify ▼            shogun_mcp::plan_source（CLI/MCP/REST 面）
   → BillingState ──► resolve_plan(trial_stamp, billing) ──► entitlements()
```

**カード情報はアプリに一切入らない**（FR-BIL-07）。Checkout も Customer Portal も Stripe ホスト型を**システムブラウザ**で開く。

---

## 2. サーバー側（`apps/website`）

Next.js の API ルートに同居させた（別サービスを立てない）。理由: 課金は「LP でも押せる」ことが要件で、既に Postgres + Drizzle が動いている場所がここだから。

### 2.1 テーブル（`src/db/schema.ts` / `src/db/migrate.ts`、追加のみ）

| テーブル | 役割 |
|---|---|
| `billing_customers` | email ↔ Stripe customer の 1:1 |
| `subscriptions` | Stripe Subscription のミラー。`status` / `current_period_end` / `cancel_at_period_end` など。「誰がいつまで使えるか」は**このテーブル1本の SELECT で答える**（Stripe API を叩かない） |
| `licenses` | デバイスが提示する資格情報。subscription 1件につき 1件。`last_device_id` / `device_count` は座席乱用のシグナル（デバイス一覧は保持しない） |
| `stripe_events` | Webhook 冪等性。処理済み event id |

**ロールバック**: `DROP TABLE stripe_events, licenses, subscriptions, billing_customers;`。既存の waitlist エンジンはこれらを参照しないため、落としても LP は動く。

### 2.2 ルート

| ルート | 中身 |
|---|---|
| `POST /api/stripe/checkout` | `{ plan, interval, email?, source? }` → `{ url }`。**Price ID は env からサーバー側で解決**（改ざん防止）。未設定なら 503（誤課金より停止） |
| `POST /api/stripe/portal` | `{ license_key }` → `{ url }`。解約・プラン変更・カード変更は全部ここ（運用の90%オフロード） |
| `POST /api/stripe/webhook` | 署名検証必須 → event id を claim（冪等）→ ハンドラ。失敗時は claim を**解放**してから 500（Stripe の再送を殺さない） |
| `POST /api/license/verify` | FR-BIL-08。`{ license_key, device_id, app_version }` → 署名付きトークン |
| `/billing/success` | ライセンスキーを提示する唯一の画面。Webhook 未達なら「取得中・再読込」を出す（`not_found` は出さない） |

受信イベント: `checkout.session.completed` / `customer.subscription.created|updated|deleted` / `invoice.payment_succeeded|failed`。

Webhook の3原則（コード内コメントにも明記）:
1. **署名検証必須**。`STRIPE_WEBHOOK_SECRET` 未設定は 500 であってバイパスではない
2. **冪等**。再送された `checkout.session.completed` が2本目のライセンスキーを発行しない（`licenses.stripe_subscription_id` の一意制約 + `ON CONFLICT DO NOTHING`）
3. **順序非依存**。各ハンドラは差分適用ではなく「現在の subscription 全体」を upsert する。`subscription.updated` が `checkout.session.completed` より先に届くのは異常ではなく通常

### 2.3 エンタイトルメント判定（`src/lib/billing.ts`、純粋関数）

- `active` / `trialing` → 有効
- `past_due` → **Stripe のリトライ期間 + 期間終了から7日間は有効**（FR-BIL-09）。カード1回失敗で有料ユーザーを止めるのがこのフロー最大の偽陰性
- `canceled` / `unpaid` / `incomplete*` / `paused` → 無効
- **未知の Price（`plan === null`）は常に無効**。推測して Pro を配らない

---

## 3. ライセンスキーとライセンストークン（2つの別物）

| | ライセンスキー | ライセンストークン |
|---|---|---|
| 形 | `shogun-XXXX-XXXX-XXXX-XXXX`（80bit CSPRNG、I/L/O/U を含まない字母） | `v1.<b64url(payload)>.<b64url(ed25519 sig)>` |
| 秘匿 | **秘密**。Keychain のみ（NFR-SEC-01）。ログにはフィンガープリント（sha256 先頭12桁）だけ | 秘密ではない（下記 §5） |
| 用途 | ライセンスAPIへのベアラ | 「このデバイスはプランXを期限Yまで」の**オフライン検証可能な主張** |
| 寿命 | 無期限（失効可） | 26時間 + オフライン猶予14日 |

トークン payload に入るのは `lic / plan / status / device / iat / exp / period_end / cancel_at_period_end / grace_days` のみ。**メール・氏名・キャプチャ内容・メモリ内容は入らない**（FR-BIL-08 / NFR-PRV-04。テストで鍵集合を固定）。

鍵生成: `node scripts/gen-license-keypair.mjs` → 秘密鍵は `LICENSE_SIGNING_KEY`（ライセンスAPI環境のみ）、公開鍵は `crates/shogun-license/src/lib.rs` の `EMBEDDED_PUBLIC_KEY_B64` に貼る。

**鍵ローテーション手順（順序が重要）**: 新公開鍵を持つビルドを**先に**配布 → 全端末が更新されたのを確認 → API の署名鍵を切り替える。逆順にすると全端末が一斉にオフライン猶予へ落ちる。

---

## 4. デバイス側（Rust）

### 4.1 新クレート `crates/shogun-license`（純粋・Linuxでテスト可能）

`verify(token, public_key, device_id) -> LicenseToken` と `LicenseToken::freshness(now_ms)`。時計は常に引数（リポジトリ規約）。

- **Fresh**: `now <= exp`
- **Grace**: `exp < now < iat + grace_days`（既定14日、`MAX_GRACE_DAYS = 30` でクランプ）。`days_offline >= 7` で **amber**（FR-BIL-09 の「7日目からアンバー」）
- **Stale**: それ以降 → `BillingState::Lapsed`（＝トライアル規則へフォールバック。刻印から7日超なら期限切れ）

設計上の判断:
- **デバイス束縛**: `device` は署名対象の中にある。別 Mac にコピーしても device_id が違うので落ちる
- **未知の plan 名は `BadPayload`**。将来プランが増えても「知らないものは Pro 扱い」にしない
- **時計を巻き戻した端末は Fresh 扱い**。NTP/タイムゾーンのブレで有料ユーザーを締め出す方が損失が大きく、巻き戻しは猶予期限を延ばせない（deadline は `iat` 起点の絶対値）
- `verify_strict` を使う（small-order 鍵・非正規署名の malleability を弾く）

新クレートにした理由: shogun-agents（依存ゼロの最下層・permission model）に ed25519/base64/serde_json を持ち込みたくないため。依存方向は `license → agents`、`mcp → license`、`desktop → license`。

### 4.2 スタブ置換（#97 §5 が予告していた2箇所）

1. `apps/desktop/src-tauri/src/entitlement.rs` `mac::current` → `crate::billing::mac::state(app)`（ローカルファイル読み + 署名検証のみ。ネットワークなし）
2. `crates/shogun-mcp/src/plan_source.rs` `FilePlanSource` → `billing.json` を毎回再読込して検証

`resolve_plan(trial_stamp, billing)` は元から billing 優先なので、**このコミットで entitlement のロジック自体は1行も変えていない**。

### 4.3 `apps/desktop/src-tauri/src/billing.rs`

コマンド: `billing_status` / `billing_activate` / `billing_refresh` / `billing_deactivate` / `billing_open_checkout` / `billing_open_portal`。検証ループは起動時 + 24時間ごと（FR-BIL-08）、別スレッド（ライセンスAPIが遅くてもパネル起動を遅らせない）。

**失敗ポリシー（実装の要）**: ライセンスAPIの応答を `Outcome { Entitled / NotEntitled / Gone / Transient }` という**値**に落とし、副作用は呼び出し側が決める。

| 呼び出し | Entitled | NotEntitled（解約・未払い） | Gone（404・失効） | Transient（オフライン・5xx） |
|---|---|---|---|---|
| `billing_activate` | キーをKeychainへ + トークン保存 | 何も書かない | 何も書かない | 何も書かない |
| `billing_refresh` / 24h ループ | トークン更新 | トークン破棄（即ロック） | トークン破棄 + キー削除 | **何もしない**（猶予窓で継続） |

この分離が要る理由: 副作用をAPI呼び出し関数の中に畳み込むと、**打ち間違えたキーを1回入力しただけで、直前まで動いていたライセンスが消える**。`billing_activate` は必ず「検証してから書く」。

「オフラインはキャンセルではない」——Transient で何も壊さないことが FR-BIL-09 の実体。

### 4.4 HTTP は shogun-core に置く

`crates/shogun-core/src/license_client.rs`（feature `net`）。FR-TR-03 の「生の HTTP クライアントは shogun-core に1つだけ」を守る。課金通信はトレーサビリティ台帳の対象外（キャプチャ内容を含まないため。§7.7 の表）だが、証明書検証は当然有効のまま。

---

## 5. `billing.json` に署名付きトークンを置くことについて（明示的判断）

NFR-SEC-01 は「ライセンストークンを含む secrets は Keychain 以外に保存しない」と書いている。本実装は **ライセンスキーは Keychain のみ**（例外なし）とした上で、**署名付きトークンはアプリデータ配下の `billing.json` にミラーする**。

理由:
- トークンは公開鍵で検証される**改竄検知可能な主張**であり、デバイス束縛かつ期限付き。他所へコピーしても何も解錠しない
- スタンドアロンの `shogun-api` / `shogun-mcp` / CLI は**デスクトップアプリの Keychain アイテムを読めない**。ここを塞ぐと「Pro を払っているのに `shogun` CLI と Memory API だけロックされる」という、はるかに悪い結果になる
- 改竄しても署名で落ちる。削除すればトライアル規則に戻るだけ（＝ユーザーが自分に不利にできるだけ）

**要オーナー確認**: NFR-SEC-01 の文言を「ライセンス**キー**は Keychain のみ、署名付きトークンはミラー可」に更新するのが妥当と考える。反対なら代替は「デスクトップがローカルIPCでプランを配る」だが、常駐プロセス前提になり CLI 単独起動が壊れる。

---

## 6. まだ入っていないもの（意図的）

- **本番の署名鍵**: `EMBEDDED_PUBLIC_KEY_B64` は空。埋めるまで `SHOGUN_LICENSE_PUBKEY` 環境変数で動く（dev/CI）。**空のまま配布ビルドを作らないこと**（全端末がトークンを検証できず、実質 #97 のロック状態になる）
- **Stripe の Product / Price の作成**: ダッシュボード作業。`STRIPE_PRICE_*` に ID を入れるまで checkout は 503
- **LP の実 Checkout 導線**: `NEXT_PUBLIC_BILLING_ENABLED=1` を立てるまで従来の waitlist CTA のまま（招待制のうちは踏ませない）。LP の CTA は年払いのみ。月払いは**アプリ内 Plan & billing** と Customer Portal から
- **領収メールでのキー送付**: 現状はチェックアウト成功ページのみ。メール送信基盤は別イシュー
- **年額 ⇄ 月額のアプリ内変更**: Customer Portal に任せる（プラン変更は Portal の許可操作に含める設定が必要）
- **MRR ダッシュボード等の分析**: issue #8 の Non Goal

## 7. 未決事項への対応

| 項目 | 対応 |
|---|---|
| OPEN-01（トライアル開始時のクレカ要否） | FR-BIL-06 どおり**フラグ化**。`STRIPE_TRIAL_DAYS=0`（既定）= ローカル7日トライアルが唯一のトライアルで、購入は即課金。`=7` = カード登録あり・7日後課金の LP ファネル。コード変更なしで切り替わる |
| OPEN-05（価格ローカライズ） | 未決のまま。`CURRENCY = 'usd'` を1箇所に固定して将来の分岐点を作った |
| 旧 #46 移行デバイス（刻印なし） | 変更なし。刻印なし = トライアル未開始 = フルアクセスのまま（#97 の既定）。課金すればトークンが勝つので実害が出るのは「刻印なしのまま無課金で使い続ける端末」だけ |

## 8. テスト

すべて Linux で走る。

- `crates/shogun-license`（12件）: 署名検証、payload 改竄、別デバイス、別鍵、不正形式、未知プラン、**猶予窓の境界**（`exp` 直前/直後、7日 amber、`iat+14d` ちょうどで Stale）、時計巻き戻し、`grace_days` クランプ、不正トークン = Unknown
- `crates/shogun-mcp::plan_source`（8件）: `billing.json` の往復、壊れたファイル = 課金レコードなし、検証不能トークンはプランを与えない、**有効な Pro トークンが期限切れトライアルに勝つ**（end-to-end）
- `crates/shogun-core::license_client`（6件）: エンドポイント組み立て、entitled/lapsed 応答、エラー body、terminal の判定、ホスト型URL応答
- `apps/website/tests/billing.test.ts`（14件）: **価格カタログが LP と一致**（年額 = 月額×12 も）、Price ID の双方向解決、未知 Price はプランなし、subscription マッピング（新旧 Stripe API の period 位置）、ステータス別エンタイトルメント + past_due 7日境界、ライセンスキーの形/字母/正規化/フィンガープリント、**署名が送信バイト列そのものを覆う**こと、トークンに個人情報が入らないこと

未実施（要デバイス/要 Stripe テストモード）: トライアル満了 → 課金 → 復元の E2E（§6.12 受け入れ基準）、Webhook 実配送、Keychain 読み書き。
