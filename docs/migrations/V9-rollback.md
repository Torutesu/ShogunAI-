# V9__transcript_segments.sql — ロールバック手順

対象: `transcript_segments` テーブルの新設（FR-MT-13）。

## 影響範囲

additive のみ。新規テーブル1つ＋索引1つで、既存テーブルへの変更はない。

## ロールバック

```sql
BEGIN;
DROP INDEX IF EXISTS idx_transcript_session;
DROP TABLE IF EXISTS transcript_segments;
DELETE FROM refinery_schema_history WHERE version = 9;
COMMIT;
```

## データ損失

失われるのは**文字起こしテキストのみ**。音声そのものは一切保存されておらず
（不変条件2：波形は RAM のリングバッファにだけ存在し ASR 後に破棄される）、
このロールバックで消えるのは波形から再生成できる派生テキストである。
ユーザーが手で書いたノート（V8 `session_notes`）はこのテーブルとは別で、
このロールバックの影響を受けない。

## 注意

V7（`sessions`）を参照する。V7 をロールバックする場合は V9 を先に落とす。
V8（`session_notes`）とは互いに独立で、順序の制約はない。
