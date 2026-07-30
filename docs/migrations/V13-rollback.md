# V13__session_notes_enhanced.sql — ロールバック手順

対象: `session_notes_enhanced` テーブルの新設（FR-MTUX-03。会議ノートの清書層）。

## 影響範囲

additive のみ。新規テーブル1つで、既存テーブルへの変更はない。
**ユーザーが書いた原文（`session_notes`）はこのテーブルとは別物**であり、本マイグレーションの
ロールバックで原文が失われることはない（それがこの2層設計の目的そのもの）。

## ロールバック

```sql
BEGIN;
DROP TABLE IF EXISTS session_notes_enhanced;
DELETE FROM refinery_schema_history WHERE version = 13;
COMMIT;
```

## データ損失

清書版（モデル生成）は**失われる**。ただしこれは原文＋文字起こしから再生成できる派生物であり、
再生成には Select KK キーの Batch レーンが必要（FR-MT-16 と同一ジョブ）。
原文は無傷なので、ユーザーの一次データの損失はない。

## 注意

V7（`sessions`）に依存する。V7 をロールバックする場合は V13 を先に落とす。
`session_notes`（V8）とは独立で、どちらか一方だけを落とせる。
