# Batch中継エンドポイント 設計

Select KKキー（Dream Cycle・Morning Brief・インデックス/分類）の供給方式。

- **決定日**: 2026-07-25
- **状態**: 実装済み（`apps/api`）。§4.1 のトークン形式は 2026-08-13 の監査で実装に合わせて確定
- **未了**: 署名鍵ペアの発番と配置（`scripts/gen-license-keypair.mjs` →
  ライセンスAPIの署名鍵 / `crates/shogun-license` の `EMBEDDED_PUBLIC_KEY_B64` /
  中継の `LICENSE_PUBKEY_B64`）。これが済むまでライセンス認証は fail-closed で全落ちする
- **関連**: CLAUDE.md 不変条件3・5・7 / requirements-v1.0.md §6.7, §6.12, FR-DC-02, FR-MB-01, FR-BIL-08, NFR-PRV-04

---

## 1. 何を決めたか

**運営のAnthropic APIキーをアプリに同梱しない。** 端末が持つのはライセンストークンだけで、
Batch API呼び出しはSelect KKが運用する中継エンドポイントが行う。

```
Mac (SHOGUN)                    relay.shogun.app              Anthropic
     │                                │                            │
     │ POST /v1/batch                 │                            │
     │  Authorization: <license token>│                            │
     │  body: 処理用チャンク[]  ──────→ │ ① トークン検証             │
     │                                │ ② プラン・上限チェック      │
     │                                │ ③ 従量計上                 │
     │                                │ ④ 運営キーで委譲 ─────────→ │ Batch API
     │                                │                            │
     │ ←──────────────────────────── 結果（custom_id で対応づけ）  │
```

## 2. なぜこの形か

### 2.1 却下した案

| 案 | 却下理由 |
|---|---|
| 運営キーをバイナリに同梱 | `strings` かプロキシで誰でも抽出できる。1人漏れれば運営アカウントで無制限に課金される |
| 同上（難読化する） | 同じ。難読化はコストを上げるだけで、抽出を防げない |
| 顧客ごとにAnthropicキーを発行して配布 | 端末に本物のキーが渡る点は変わらない。アプリ外での利用を防げず、運用も重い |
| Dream CycleもBYOKにする | Standardの「BYOK不要で全機能が動く」（§6.12.1）が崩れる。価格設計のやり直しになる |
| 分類をローカル小型モデルで | 「英語で精度を最優先」という方針と衝突する。embeddingと違い、抽出精度の差が製品価値に直結する |

### 2.2 中継にした決定的な理由

漏洩リスクは分かりやすい方だが、**より重いのは原価の上限強制**である。

Standardは月額固定（$49）、Batchのコストは処理量に比例する。鍵が端末側にあると、
「今月の上限に達したので止める」という判断を**サーバー側で強制できない**。アプリを改造すれば
無視できてしまう。requirements-v1.0.md のリスク表にある

> Batch APIのコスト増大（Select KKキー負担）→ チャンク量の上限設計・Dream Cycle処理量のメトリクス監視

は、鍵が端末にある前提では実装不可能な対策になる。中継にすると上限も計上もサーバー側の
単純な処理になる。

## 3. 要件との整合（判断が必要だった点）

### 3.1 「中間サーバーを経由しない」との関係

FR-INT-01 は**第1層連携**について「中間サーバー（Select KK運営サーバー含む）を経由しない」と
定めている。これは各サービス（Gmail等）の公式リモートMCPに直接つなぐという規定であり、
**Batchチャンクの経路については何も定めていない**。要件の穴だったので、ここで埋める:

> **Batchレーンに限り**、Select KKが運用する中継を経由する。第1層連携（OAuth・MCP）は
> 従来どおり直結を維持し、中継を通さない。

### 3.2 プライバシー（NFR-PRV-04）との関係

NFR-PRV-04 は「サーバー側にメモリ内容・state内容を**保存しない**」と定めている。
中継は**素通し**であり、以下を満たすことで整合する:

- チャンク本文をログ・DB・監視系に**書かない**（記録するのはバイト数・件数・ハッシュのみ）
- Anthropicのレスポンスも保持しない。取得次第、端末に返して破棄する
- 保持するのは課金・上限判定に必要な**集計値のみ**（アカウントID・日時・件数・トークン数）

### 3.3 トレーサビリティ（AR-11 / 不変条件3）

現在の `traceability_log` は `route` に `batch_api` を持つ。中継経由は経路が変わるので、
**区別できる値を持たせる**:

| `route` | 意味 |
|---|---|
| `batch_api` | Anthropicへ直接（BYOK運用・開発時） |
| `batch_relay` | Select KK中継経由（本番のStandard/Pro） |

トレーサビリティ画面には、Composioの「第三者経由」と同じ扱いで**「運営サーバー経由」を明示**する。
ユーザーが自分のデータの経路を誤解しない状態を保つ（不変条件3）。

## 4. API仕様

### 4.1 認証

`Authorization: Bearer <license token>`。FR-BIL-08 の署名付きライセンストークンをそのまま使う
（プラン・有効期限を含む）。**新しい認証系を作らない。**

#### トークン形式（実装確定 2026-08-13）

JWT ではない。`apps/website/src/lib/license.ts` が発行し、`crates/shogun-license` と
`apps/api/src/auth.ts` が同じ形を検証する自前の3分割トークン:

```
v1.<base64url(payload JSON)>.<base64url(Ed25519 signature)>
```

署名対象は `v1.<payload>` の**そのままのバイト列**。検証側は base64url を再エンコードして
`timingSafeEqual` で突き合わせ、代替エンコードを受け付けない。payload:

```jsonc
{
  "v": 1,
  "lic": "lic_…",          // ライセンスID（= 中継の所有者キー）
  "plan": "standard" | "pro",
  "status": "active" | "trialing" | "past_due",
  "device": "…",           // 端末バインド
  "exp": 1760000000        // unix 秒。約24時間
}
```

- 検証はライセンスAPIと同じ Ed25519 公開鍵で、中継側がローカル検証する（往復を増やさない）。
  公開鍵は中継の `LICENSE_PUBKEY_B64`（raw 32バイトの base64）に置き、SPKI へ包んで読む。
  **公開鍵が無ければ全リクエストを 401 で落とす（fail-closed）。**
- 失効は短い有効期限（24h）＋ライセンスAPIでの再取得で回す
- 端末側で中継に提示するトークンは `license_client::cached_license_token()` が返す
  `billing.json` のキャッシュ。**これは不変条件7 に対する明示的例外**（CLAUDE.md 参照）:
  署名済み・device バインド・24時間で失効するため秘密ではない。
  **ライセンスキー本体（`shogun-XXXX-…`、ライセンスAPIの bearer）は引き続き Keychain のみ。**

#### 所有権チェック

`GET /v1/batch/{id}` は**そのライセンスが作ったバッチしか読み出せない**。Anthropic の
batch id は推測可能なので、これが無いと任意の licensee が他人の Dream Cycle 結果を
ストリームできる。「存在しない」と「他人のもの」は同じ 404 を返し、oracle を作らない。
所有者は `UsageStore.attachBatch` が計上と同時に記録する。

### 4.2 `POST /v1/batch`

```jsonc
// request
{
  "purpose": "consolidation",      // traceability の purpose と同じ語彙
  "model_class": "classify",       // モデルIDは端末が決めない（4.4）
  "items": [
    { "custom_id": "1234", "chunk": "…処理用チャンク…" }
  ]
}
// 202 Accepted
{ "batch_id": "rb_…", "accepted": 812 }
```

### 4.3 `GET /v1/batch/{id}`

```jsonc
// 200 — 実行中
{ "status": "in_progress", "completed": 300, "total": 812 }
// 200 — 完了
{ "status": "ended",
  "results": [ { "custom_id": "1234", "text": "{\"commitments\":[…]}" } ] }
```

`custom_id` はイベントIDで、`parse_batch_classification` がそのまま読める形を維持する。

### 4.4 モデルIDは端末が決めない

現在の `dream.rs` は `BATCH_MODEL` を定数で持っているが、中継化したらこれは**サーバーが決める**。
端末がモデルを指定できると、原価の高いモデルを要求される。端末が送るのは `model_class`
（`classify` / `summarize` / `brief`）という**意図**だけにする。

### 4.5 エラーと上限

| HTTP | 意味 | 端末の挙動 |
|---|---|---|
| 400 | 本文が JSON でない・スキーマ違反・上限超過（下表） | バグ。台帳に記録し、リトライしない |
| 401 | トークン無効・失効・公開鍵未設定 | ライセンス再検証。オフライン猶予（FR-BIL-09）中はローカルレーンに落ちる |
| 402 | プラン外 | ローカルレーンで継続（機能は止めない） |
| 404 | 不明なバッチID **または他ライセンスのバッチ** | 取得を諦める。両者を区別しない（§4.1 所有権チェック） |
| 413 | 本文が 8 MiB 超 | チャンクを分割して再送 |
| 429 | 当日の上限到達 **またはレート制限** | 当夜はローカルレーンに落とし、インジケータはamber |
| 503 | 計上台帳が読めず上限を強制できない | 一時障害として扱い、翌晩に持ち越す |
| 5xx | 中継/上流の障害（上流本文は返さない。502 + 固定文字列） | サイクル失敗として台帳に記録。翌晩に持ち越す（FR-DC-05） |

**いずれの場合もローカル機能（キャプチャ・検索・Fusion提示）は無影響**（FR-DC-05）。

#### 入力上限（`apps/api/src/types.ts` / `app.ts`）

| 項目 | 上限 |
|---|---|
| 本文全体 | 8 MiB（ハンドラ到達前に `bodyLimit` で 413。無制限の `c.req.json()` は1リクエストOOM） |
| `items` 数 | 1,000 |
| `chunk` 1件 | 256 KiB |
| `custom_id` | 256 バイト |

#### 上限の強制は reserve-then-commit

日次チャンク上限は**上流呼び出しの前に予約し、失敗したら解放する**。read-then-write だと
N 本の同時投入がすべて同じ `used` を読んで全部通る。`JsonFileUsageStore` は read-modify-write を
1本のキューに直列化し、台帳が壊れて読めない場合は「0扱いで通す」のではなく `unavailable` →
503 を返す（**上限を強制できないなら使わせない側に倒す**）。
回帰テストは「上限10に対して同時50投入 → ちょうど10だけ 202」（`apps/api/test/hardening.test.ts`）。

## 5. 端末側の実装差分

現行コードからの差分は小さい。`AnthropicBatchClient` は `base_url` と資格情報を持つ形に
すでになっている。

| 箇所 | 変更 |
|---|---|
| `crates/shogun-core/src/llm/mod.rs` | `SelectKkKey` は「Batchレーンの資格情報」として意味が変わる（型はそのまま。Agent側と混ざらない型分離は維持） |
| `crates/shogun-core/src/llm/anthropic.rs` | 中継のリクエスト/レスポンス形に対応する `BatchRelayClient`。`AnthropicBatchClient` は開発・直結用に残す |
| `traceability` | `Route::BatchRelay` を追加。DBのCHECK制約に値を足す追加マイグレーション |
| `apps/desktop/.../dream.rs` | Keychain account を `select-kk-batch` → ライセンストークンの口に変更。`BATCH_MODEL` 定数を削除し `model_class` を送る |
| Settings UI | トレーサビリティ画面に「運営サーバー経由」の表示 |

## 6. 未決事項

| ID | 内容 | 期限 |
|---|---|---|
| OPEN-B1 | 中継のホスティング先とリージョン（データ経路の開示文面に影響する） | 実装着手前 |
| OPEN-B2 | プランごとの月次チャンク上限の具体値。原価モデルから逆算する | 価格確定時 |
| OPEN-B3 | オフライン猶予（14日）中のBatch可否。現案は「ローカルレーンに落とす」 | 実装着手前 |
| OPEN-B4 | 分類結果のキャッシュ（同一チャンクの再送を中継側で弾くか） | 原価が問題になってから |

## 7. 移行までの暫定運用

中継が動くまでは、現行の直結パスをそのまま使う。開発者が自分のAnthropicキーを
Keychainに置けば Batchレーンが動き、置かなければローカルルールレーンで夜間サイクルが回る。

```bash
security add-generic-password -s SHOGUN -a select-kk-batch -w 'sk-ant-…' -U
```

**この暫定運用を配布ビルドに持ち込まないこと。** 配布時点で鍵が端末に必要な状態なら、
それは §2.1 で却下した案そのものである。

---

## 付録: BYOKキーが複数ある場合の扱い（確定 2026-07-25）

Batchレーン（本文）と対になる、**Agentレーン側**の鍵の扱い。質問「複数Keyがある時はどういう
処理をするか設計できてる?」への回答をここに固定する。

### 保存

**プロバイダごとに独立したKeychainエントリ。** 1つのスロットを共有しない。

| プロバイダ | Keychain account | ベースURL |
|---|---|---|
| Anthropic | `anthropic-byok` | api.anthropic.com |
| OpenRouter | `openrouter-byok` | openrouter.ai/api/v1 |
| OpenAI | `openai-byok` | api.openai.com/v1 |
| Gemini | `gemini-byok` | generativelanguage.googleapis.com/v1beta/openai |

プロバイダを切り替えても他の鍵は**消えない**。4つ全部入れて行き来できる。
（不変条件7: 平文ファイル・DB・ログには一切書かない）

### 選択

**同時に有効なのは1つだけ。** 設定の Model セクションで選んだプロバイダが、チャットと
⌥タップの両方に使われる。「どれか使えるやつ」を探す挙動はしない。

### 自動フォールバックを実装しない理由

1. **課金の予測可能性。** ユーザーが選んでいないプロバイダに勝手に課金しない。1回のフォールバックが
   月末の請求書で初めて分かる、という事態を作らない
2. **モデルIDはプロバイダ固有。** `claude-sonnet-5` と `anthropic/claude-sonnet-4.5` と
   `gemini-2.5-flash` は互換性がない。フォールバックはモデル設定も一緒に切り替えることになり、
   ユーザーが指定したモデルが黙って別物になる
3. **出力の一貫性。** 同じ質問が日によって別のモデルから返るメモリ製品は、挙動が読めない
4. **失敗の可視性。** フォールバックは失敗を隠す。鍵が無効なら、そう言うべき

### 失敗したとき

401/403（鍵が拒否された）は `LlmError::Unauthorized` として**他のエラーと型で区別する**。
リトライしても直らない唯一の失敗であり、ユーザーが直せる唯一の失敗でもあるため:

- Agentレーン: `key_rejected` フラグを立て、設定の Key セクションに
  「This key was rejected. Check it, or pick another provider.」を出す。
  鍵かプロバイダを変更するとクリアされる
- Batchレーン: 夜間サイクルはローカルレーンに落ちる（§4.5）。失敗として記録しない
- **どちらもフォールバックしない。** 別のプロバイダに自動で切り替えることはしない

これが無かった時の実挙動: ⌥タップが401を返し、何も挿入されず、ログ以外どこにも出なかった。
ユーザーから見ればショートカットが壊れているのと区別がつかず、5回連続で押されていた。

### v1スコープとの差異（記録）

requirements-v1.0.md は「BYOKはv1でAnthropicのみ（プロバイダ抽象化層は用意、実装は1つ）」
（ADR-002）としているが、実装は OpenAI互換クライアント経由で OpenRouter / OpenAI / Gemini に
広がっている。抽象化層は1つ（`OpenAiCompatAgentClient`）のままで、増えたのは
**ベースURLとデフォルトモデルのテーブル行だけ**なので、ADR-002 の意図（クライアント実装を
乱立させない）には反していない。要件本文の更新は次のスコープ見直しで行う。
