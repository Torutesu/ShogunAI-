# V10__meeting_recaps.sql — ロールバック手順

対象: `meeting_recaps` テーブルの新設（MT4 / §6.16、生成された議事録）。

## 影響範囲

additive のみ。新規テーブル1つ。`sessions(id)` を参照する（UNIQUE session_id、1会議1レコード）。

## ロールバック

```sql
BEGIN;
DROP TABLE IF EXISTS meeting_recaps;
DELETE FROM refinery_schema_history WHERE version = 10;
COMMIT;
```

## データ損失

失われるのは**モデルが生成した要約・決定事項・ネクストアクション**のみ。元になった
文字起こし（V9 `transcript_segments`）とユーザー自身のノート（V8 `session_notes`）は
別テーブルで、影響を受けない。Recap は同じ入力から再生成できる。

## 注意

V7（`sessions`）を参照する。V7 をロールバックする場合は V10 を先に落とすこと。
