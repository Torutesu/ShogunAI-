# V12__action_feedback.sql — ロールバック手順

対象: `action_feedback` テーブルの新設（FR-PAT-01。Patternsレイヤーの学習入力ログ）。

## 影響範囲

additive のみ。新規テーブル1つ＋インデックス1つで、既存テーブルへの変更はない。
v1では書き込みのみで、実行時にこのテーブルを読むコードパスはない（読み手は
FR-CF-03の採択率入力とv1.5のFR-PAT-02）。

## ロールバック

```sql
BEGIN;
DROP INDEX IF EXISTS idx_action_feedback_kind_ts;
DROP TABLE IF EXISTS action_feedback;
DELETE FROM refinery_schema_history WHERE version = 12;
COMMIT;
```

## データ損失

記録済みの採択・修正・却下の履歴は**失われる**。内容（キャプチャテキスト・
アクション本文）は元々含まれないため機密上の損失はないが、v1.5のPatterns学習は
記録の再開時点からやり直しになる（遡って再構成できない）。

## 注意

他テーブルへの外部キーはなく、単独で落とせる。
