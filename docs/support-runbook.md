# CS / バグ報告窓口 運用ランブック

対象: プロジェクトボード「CS/バグ報告窓口作る　対応方法や最適化も」/ Issue #108（バグ報告→修正の仕組み）。
実装は 3 面: デスクトップの Help & Support パネル（intake）、shogunaios.com の intake API（保管）、
admin API（トリアージ）。本書は「届いた報告をどう処理するか」の運用手順。

## 1. 窓口の構成

```
ユーザー (Settings → Help)
  → support_submit_report (Tauri / apps/desktop/src-tauri/src/support.rs)
      - カテゴリ・本文・任意の返信先メール
      - 診断タプル（app_version / os_version / plan）は明示チェックボックスの opt-in のみ
      - egress ledger に Route::Support で記録（不変条件3。本文は digest のみ）
  → POST https://shogunaios.com/api/support/report (apps/website)
      - レート制限 5件/時/IP、ボディ 8KB 上限、カテゴリ・長さ・メール検証
      - support_tickets 行を作成（status = open）
      - info@shogunaios.com へ通知メール（Resend。best-effort、失敗しても保存は成功）
  → GET/PATCH https://shogunaios.com/api/admin/support（x-admin-token）
```

送られる内容はユーザーが書いた本文と opt-in 診断のみ。キャプチャ内容・メモリ内容・
ライセンスキーは構造上送れない（Rust 側がフィールドを組み立てる。webview から任意
フィールドは注入できない）。

## 2. トリアージ手順（対応方法）

> **通知先: `info@shogunaios.com`。** チケット作成時に Resend 経由でメールが飛ぶ。
> 件名は `[bug] 本文の先頭60字…`、Reply-To は報告者のメール（記入があれば）なので、
> 受信箱でそのまま返信すれば本人に届く。
>
> **ただしメールは補助であって台帳ではない。** 通知は best-effort で、失敗しても
> 報告の保存は成功扱いになる（保存済みのものを「送り直してください」と言わせない
> ため）。取りこぼしを拾うのは下の admin API なので、日次確認はやめないこと。
>
> **送信が止まる条件**（どれも報告の保存自体は成功する）:
> - `RESEND_API_KEY` 未設定 → 通知は完全に無効。**本番で未設定だと誰にも届かない**
> - `SUPPORT_NOTIFY_FROM` のドメインが Resend で未検証（SPF/DKIM）→ 全通信が拒否
> - `shogunaios.com` に MX 未設定 → 送れても `info@` 側で受信できない
>
> 疑わしいときは Worker のログで `support notification rejected: HTTP <status>` を探す。

毎営業日 1 回、open チケットを見る:

```bash
curl -s https://shogunaios.com/api/admin/support?status=open \
  -H "x-admin-token: $ADMIN_TOKEN" | jq .
```

各チケットの処理:

1. **bug** — 再現可否を判断し、GitHub Issue を立てる（タイトル先頭に `[support]`、
   本文にチケット id・app_version・os_version・plan を転記。**メールアドレスは
   GitHub に書かない** — 個人情報を公開リポジトリへ持ち出さない）。Issue #108 の
   自動修正フロー（#191）へ接続するのはこの Issue 化の時点。
2. **feedback** — 機能要望は該当する既存 Issue に集約、なければ新規 Issue。
3. **question** — email があれば返信。なければドキュメント側のギャップとして記録
   （同じ質問が 2 回来たら docs / LP FAQ の修正 Issue を立てる）。

処理したら status を進める:

```bash
curl -s -X PATCH https://shogunaios.com/api/admin/support \
  -H "x-admin-token: $ADMIN_TOKEN" -H "content-type: application/json" \
  -d '{"id":"<ticket uuid>","status":"triaged"}'
```

- `open` → 未読。`triaged` → Issue 化 or 返信済みで対応が走っている。`resolved` → 完了
  （修正リリース済み / 回答済み / 対応不要と判断）。
- SLA 目安: open → triaged は **2 営業日以内**。bug で report が「使えない」級
  （起動不能・データ消失疑い）は当日。

## 3. 最適化ループ

週次で見るもの:

- **カテゴリ別件数**（`status` 無指定で取得して集計）。bug が急増したリリースは
  リリースノートと突き合わせる。
- **同一 digest / 同内容の重複**: 同じ症状が 3 件来たら、その症状は「サポートの問題」
  ではなく「プロダクトの問題」。優先度を上げて Issue 化する。
- **question の傾向**: 回答で済ませ続けている質問はオンボーディング / docs の欠陥。
  docs 側の Issue に変換して question の流入自体を減らす。
- レート制限（5件/時/IP）と 8KB 上限がスパムの一次防御。荒らしが観測されたら
  `ip_hash` で照合し、`rate-limit` のバケット値を絞る（コード変更）。

## 4. プライバシー境界（変えないこと）

- チケット本文はユーザー著作のテキスト。保存先は **Postgres（運営 DB）のみ**、
  加えて **info@shogunaios.com への通知メール**にだけ載る。分析イベント（PostHog）
  には件数以外を載せない。
- email は返信のためだけに使う。通知メールの Reply-To に入るのはこの用途。
  GitHub Issue・ログ・分析へ転記しない。
- デスクトップ側は送信成功時に traceability_log へ 1 行（route = `support`、
  digest のみ）。ロールバックは docs/migrations/V21-rollback.md。
- 診断タプルは opt-in。デフォルト ON のチェックボックスだが、外して送れば
  3 フィールドとも送信されない（サーバー側も nullable）。

## 5. 障害時

- intake が 500 を返す: Postgres 接続（Hyperdrive）を疑う。デスクトップ側は
  エラーをそのまま表示するだけでアプリ動作に影響しない。
- レートリミッタは fail-open（可用性優先）。DB 落ちでも報告自体は失敗する点に注意
  （insert が本体のため）。復旧後の再送はユーザー任せ — 失敗時 UI は本文を消さない。
