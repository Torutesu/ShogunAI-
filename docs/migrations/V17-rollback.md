# V17__lesson_distill_meta.sql — ロールバック手順

対象: `lesson_distill_meta`（LessonDistillation のウォーターマーク、Plan D-4）。

## 影響範囲

additive のみ。単一行テーブル（`id` を 1 に CHECK）1つ、索引なし、外部キー参照なし。
初期値 `last_processed_feedback_id = 0`（＝未処理）を INSERT 済み。

## ロールバック

```sql
BEGIN;
DROP TABLE IF EXISTS lesson_distill_meta;
DELETE FROM refinery_schema_history WHERE version = 17;
COMMIT;
```

（SQL ヘッダのインライン手順は `DROP TABLE lesson_distill_meta;` のみだったが、
`refinery_schema_history` の行も消さないと再適用されない。）

## データ損失

進捗ウォーターマーク1個だけ。`feedback_events` / `lessons` 本体は無傷。

失うのは「どこまで蒸留したか」であり、これが消えると次回の蒸留パスが
`feedback_events` を先頭から読み直す。**結果は壊れない** — lesson の upsert は
evidence を重複排除するので、同じ feedback を再読しても evidence_count は二重加算されない。
コストが一度だけ余計にかかるだけ。

## 注意

- V16 の3テーブルに論理的に依存する。**V16 を戻すなら V17 を先に戻すこと。**
- 逆方向（V17 だけ戻して V16 を残す）は安全。蒸留ジョブを V17 以前へ戻せば動く。
- 再適用（V17 を入れ直す）と `last_processed_feedback_id` は 0 に戻る。上記のとおり
  再読は安全だが、`feedback_events` が大きい環境では初回の Dream パスが長くなる。
