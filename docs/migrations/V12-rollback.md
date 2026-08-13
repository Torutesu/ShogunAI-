# V12__screen_frames.sql — ロールバック手順

対象: `screen_frames`（Visual recall のフレームキャッシュ、issue #106 / 2026-08-02 のオーナー決定）。

## 影響範囲

additive のみ。新規テーブル1つ＋索引1つ。`event_log(id)` を参照する。

## ロールバック

```sql
BEGIN;
DROP INDEX IF EXISTS idx_screen_frames_created;
DROP TABLE IF EXISTS screen_frames;
DELETE FROM refinery_schema_history WHERE version = 12;
COMMIT;
```

## データ損失

**このロールバックは「消えすぎる」側に倒れており、それが正しい。** このテーブルは不変条件2に
対する明示的例外（Visual recall On のとき、OCR 用の圧縮 JPEG を暗号化 DB に最大72時間だけ保持）
の置き場そのものなので、落とせば画像は残らない。OCR で取り出したテキストと provenance は
`event_log`（`source = screen_ocr`）にあり、そちらは無傷。

## 注意

72時間の自動削除は `screen_frames::purge_older_than`（`daemon` から駆動）。ロールバック後は
この purge 対象が存在しなくなるので、呼び出し側も V12 以前に戻すこと。
