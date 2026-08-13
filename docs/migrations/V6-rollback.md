# V6__base_confidence.sql — ロールバック手順

対象: state テーブル4つ（`people` / `projects` / `commitments` / `open_loops`）への
`base_confidence` 列追加（FR-ST-21、confidence 減衰を「累積」から「再計算可能」へ）。

## 影響範囲

additive のみ。4つの NOT NULL DEFAULT 0.0 列。既存行はデフォルト値で埋まる。

## ロールバック

```sql
BEGIN;
ALTER TABLE people      DROP COLUMN base_confidence;
ALTER TABLE projects    DROP COLUMN base_confidence;
ALTER TABLE commitments DROP COLUMN base_confidence;
ALTER TABLE open_loops  DROP COLUMN base_confidence;
DELETE FROM refinery_schema_history WHERE version = 6;
COMMIT;
```

## データ損失

`base_confidence`（証拠から導かれる基準値）が消える。**現在の `confidence` 列は残る**が、
V6 以前のコードは減衰を毎回 stored 値に掛け直す実装なので、戻した直後から
「時間ごとに掛かる」バグ（V6 が直した当のもの）が再発する。ロールバックは
V6 以前のアプリと必ずセットで行うこと。

## 注意

減衰の再計算は `recompute::decay_confidence`。V6 以降は base × 時間関数で毎回導出するため、
`confidence` 列は導出結果のキャッシュに過ぎない。
