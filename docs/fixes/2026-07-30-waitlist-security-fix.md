# Waitlist セキュリティ修正 — 重複signupトークン漏えい / origin fail-open

- 日付: 2026-07-30
- 対象: `apps/website`（waitlist API）。監査レポート（#47/#69 の項）の High / Med 指摘への対応
- 変更ファイル:
  - `apps/website/src/app/api/waitlist/signup/route.ts`
  - `apps/website/src/lib/referral.ts`（`signupPayload` 追加）
  - `apps/website/src/lib/waitlist-auth.ts`（`isAuthorizedOrigin` fail-closed 化）
  - `apps/website/src/lib/service.ts`（コメントのみ）
  - `apps/website/tests/waitlist-security.test.ts`（新規）、`tests/e2e.ts`（追補）
  - `apps/website/.env.example`（挙動注記）

## 1. 脅威モデル

waitlist は 2 トークン設計:

| トークン | 性質 | 用途 |
|---|---|---|
| `refCode` | 公開（~60bit） | 招待リンク・リーダーボード |
| `statusToken` | **秘匿ベアラ**（192bit） | ステータス閲覧（`GET /api/waitlist/status`）＋プロフィール書き込み（`POST /api/waitlist/profile`） |

`statusToken` は事実上のアカウント認証情報。これが漏れると第三者が
(a) 待機順位・紹介実績・回答状況の閲覧、(b) nickname / 回答の改竄（＝実質乗っ取り）ができる。

### 脆弱性 1【High】重複 signup で他人の statusToken が返る

修正前の `signup/route.ts` は `addParticipant()` が `duplicate: true` で返した**既存行**の
`statusToken` から statusUrl を組み立ててそのまま返却していた。

- 攻撃者要件: 被害者のメールアドレスを知っている（推測可能）だけ。認証不要
- 攻撃手順: `POST /api/waitlist/signup {"email":"victim@x.com"}` → レスポンスの
  `statusUrl` に被害者の秘匿トークンが含まれる
- 影響: ステータス閲覧＋プロフィール改竄。レート制限（5/min/IP）内で標的型に十分成立

### 脆弱性 2【Med】origin allowlist の fail-open

修正前の `waitlist-auth.ts` は `WAITLIST_ALLOWED_ORIGINS` が未設定/空のとき
**全 origin を許可**していた（`if (allow.length === 0) return true`）。本番デプロイで
環境変数を入れ忘れた瞬間に CSRF 的なクロスサイト POST・スパム signup への防御が消える。
設定ミスが「防御ゼロ」に静かに縮退する構成は不可。

## 2. 採用した修正

### 2.1 重複 signup は汎用成功レスポンス（トークンを一切返さない）

- `addParticipant()` の `duplicate` フラグを route で参照し、重複時は
  `{ ok: true, refCode: null, statusUrl: null }` を返す。これは**既存の honeypot パスと
  同一のレスポンス形**（accept-and-drop の既定形をそのまま流用）
- 判定ロジックは純関数 `signupPayload(row, duplicate, origin)`（`lib/referral.ts`）に
  切り出し、単体テスト可能にした
- **トークンはローテーションしない**。既存所有者は元の status リンクをそのまま使い続ける
- フロント（`WaitlistForm.tsx`）は既に `statusUrl` が null のとき「You're on the list.」
  （`okListed`）を表示する分岐を持つため、**フロント変更は不要**
- `service.ts` の `ensureTokens`（トークン欠損レガシー行の補完）は維持。データ整合性の
  処理であり、レスポンスに出さない限り無害

### 2.2 認可の確認（statusToken 厳格ベアラ）

現行コードを検証した結果、修正不要であることを確認:

- `POST /api/waitlist/profile` — body の `code` を `isValidStatusToken` で形状検査後、
  `findByStatusToken` でのみ解決。**email では一切引けない**
- `GET /api/waitlist/status` — 同様に statusToken のみ。email/IP/UA は返さない
- `GET /api/waitlist/invite-context` — 公開 refCode のみ、返すのはマスク済み email と tier のみ
- 公開 refCode は正規表現の形状（6–16 文字）で statusToken（20–64 文字）として通らない
  （既存テスト「two-token split」で担保）

### 2.3 origin チェックの fail-closed 化

`isAuthorizedOrigin` の新しい判定順:

1. `WAITLIST_WEBHOOK_SECRET` が設定済みかつヘッダ一致 → 許可（サーバ間、従来どおり。
   空文字 secret は不成立）
2. `Origin` ヘッダなし → **拒否**（ブラウザの cross-site POST は必ず Origin を送る。
   サーバ呼び出しは secret を使う）
3. allowlist が設定されている → 完全一致のみ許可（同一オリジンでもリスト外なら拒否＝
   明示設定が常に優先）
4. allowlist 未設定/空 → **同一オリジンのみ許可**（`Origin` の host と リクエスト URL の
   host の一致。全許可への縮退はしない）

**ローカル開発**: `next dev`（localhost:3000）のフォーム POST は同一オリジンなので、
環境変数なしでそのまま動く。別オリジンから叩きたい場合のみ
`WAITLIST_ALLOWED_ORIGINS=http://localhost:3000` 等を `.env` に設定する（`.env.example`
に注記済み）。**本番は allowlist の明示設定を必須運用とする**（同一オリジン fallback は
host 比較のみで scheme を見ないため、明示設定が常に望ましい）。

## 3. 却下した代替案

| 代替案 | 却下理由 |
|---|---|
| 重複時にトークンを**ローテーション**して新 URL を返す | 攻撃者が被害者のメールで signup するだけで**被害者の既存リンクを無効化できる**（DoS 化）。かつ新トークンが攻撃者に渡るので漏えいも解決しない |
| 重複時に**ダミートークン**を生成して返す（完全識別不能化） | 正規の再訪ユーザーが死んだ status ページへリダイレクトされる UX 破壊。攻撃者は status API を 1 回叩けば真偽判別できるため、識別不能性の上積みはリクエスト 1 回分しかない。複雑さに見合わない |
| 重複時に**メールで status リンクを再送**（監査の推奨案の完全形） | 現時点でメール送信基盤（トランザクショナルメール）が website に存在しない。導入は本修正のスコープ外。基盤ができ次第、汎用レスポンス＋「リンクをメールで送りました」へ移行するのが理想形（フォローアップ） |
| fail-closed を「未設定なら**起動エラー**」で実装 | Next.js のルートはリクエスト時評価でビルド時に落とす自然な場所がなく、dev 体験も壊す。同一オリジン fallback の方が「安全側に倒しつつ dev が動く」を両立する |
| Supabase RLS 導入・最小権限 DB ロール | 有効な多層防御だが DB 運用変更を伴い「最小 diff」の範囲外。監査どおりサーバ専用 `DATABASE_URL` は許容。フォローアップとして残す |

## 4. テスト計画

### 単体（`pnpm test` = `tsx --test tests/*.test.ts`、DB 不要）

`tests/waitlist-security.test.ts`（新規）:

- 重複 payload に statusToken / refCode が**文字列としても**含まれない
- 重複 payload が honeypot レスポンスと deepEqual（識別不能）
- 新規 signup は従来どおり statusUrl（自分のトークン）を受け取る（happy path 不変）
- トークン欠損行は汎用 payload に縮退（壊れた URL を作らない）
- origin: 未設定×クロスオリジン→拒否 / 未設定×Origin なし→拒否 / 空文字設定→拒否 /
  未設定×同一オリジン→許可（dev 動作） / 設定済み→リスト一致のみ / `Origin: null` 等の
  不正値→例外にせず拒否 / secret 一致→許可、空 secret→不許可

### E2E（`pnpm e2e`、要 DATABASE_URL）

`tests/e2e.ts` に追補:

- 重複 signup で**トークンがローテーションされない**（既存リンク維持）
- 重複行を `signupPayload` に通すと汎用形になる（route 相当の検証）

### 手動確認（デプロイ後）

1. 新規メールで signup → status ページへ遷移（従来どおり）
2. 同じメールで再 signup → 「You're on the list.」表示のみ、レスポンスにトークンなし
3. 1 のリンクが引き続き有効（ローテーションなし）
4. `WAITLIST_ALLOWED_ORIGINS` を消した環境で `curl -H 'Origin: https://evil.example'`
   → 403、本サイトのフォームからは成功

## 5. 残存リスク / フォローアップ

- **メール登録済みかの列挙**: 重複時の汎用成功は「新規なら statusUrl あり / 既存なら null」
  の差で登録有無が判別できる（Med→Low に低減、乗っ取りは不可能に）。完全解消には
  「新規/重複ともトークンをメール送付のみ」への移行が必要 → メール基盤導入後の課題
- レートリミッタは fail-open 設計（コメント明記済み）・`rate_limits` テーブル無限成長 →
  監査 Low のまま未対応
- `WAITLIST_IP_SALT` の `'dev-salt'` フォールバック → 監査 Low のまま未対応（本番 env 必須運用）
- `console.error('signup error:', e)` に PG unique violation 経由でメールが乗り得る点、
  findByEmail→insert の TOCTOU（`onConflictDoNothing` 化）→ 監査 Low、別修正で対応推奨
- website の CI 不在（本テストが push/PR で走らない）→ 監査 Med、別 Issue 推奨
