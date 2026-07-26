# V8__session_notes.sql — ロールバック手順

対象: `session_notes` テーブルの新設（FR-MT-10）。

## 影響範囲

additive のみ。新規テーブル1つで、既存テーブルへの変更はない。

## ロールバック

```sql
BEGIN;
DROP TABLE IF EXISTS session_notes;
DELETE FROM refinery_schema_history WHERE version = 8;
COMMIT;
```

## データ損失

ユーザーが会議中に書いたノートは**失われる**。これはユーザー自身が書いた
唯一の一次データであり、要約や文字起こしと違って再生成できない。
ロールバック前に `maintenance::export_json` を実行すること。

## 注意

V7（`sessions`）に依存する。V7 をロールバックする場合は V8 を先に落とす。
