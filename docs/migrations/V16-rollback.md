# V16__lessons.sql — ロールバック手順

対象: `feedback_events` / `lessons` / `lesson_provenance`（L5 lessons、
`docs/layer-completion-designs.md` §5.1）。

## 影響範囲

additive のみ。新規テーブル3つ＋索引4つ。ただし `lesson_provenance` が
`lessons(id)` と `feedback_events(id)` を参照する**外部キーを持つ**ため、
ドロップ順序が重要。

## ロールバック

**children-first。`lesson_provenance` を先に落とす。**

```sql
BEGIN;

-- 1) 子（FK を持つ側）から
DROP TABLE IF EXISTS lesson_provenance;

-- 2) 親テーブルと索引
DROP INDEX IF EXISTS idx_lessons_active;
DROP INDEX IF EXISTS idx_lessons_scope;
DROP TABLE IF EXISTS lessons;

DROP INDEX IF EXISTS idx_feedback_events_scope;
DROP INDEX IF EXISTS idx_feedback_events_ts;
DROP TABLE IF EXISTS feedback_events;

DELETE FROM refinery_schema_history WHERE version = 16;

COMMIT;
```

親を先に落とすと `PRAGMA foreign_keys = ON` の接続で失敗する。逆順にしないこと
（`DROP TABLE IF EXISTS` は存在しない表を黙って飛ばすので、順序さえ守れば冪等）。

## データ損失

**学習信号の原本が消える。再構成できない。**

- `feedback_events` は承認時の編集・却下・無編集承認という**その瞬間にしか取れない**信号。
  `event_log` から再導出できない。
- `lessons` は蒸留結果なので feedback があれば作り直せるが、その feedback ごと消える。
- `active = 0` にした（＝ユーザーが明示的にオフにした）レッスンの意思表示も消える。

ロールバック前に3テーブルをまとめてエクスポートすること。ただし
**`before_text` / `after_text` はローカル限定のユーザー本文である。**
退避ファイルを端末外へ出さない、ログに流さない（不変条件3・ログ規約）。

## 注意

- V17（`lesson_distill_meta`）が本マイグレーションに論理的に依存している。
  **V17 を先に戻すこと**（ウォーターマークだけ残しても指す先がない）。
- ロールバック後は `LessonDistillation` の Dream ジョブと Learned UI も V16 以前へ戻す。
- V19 以降で `feedback_events` に列を足す予定がある（surface / rank、acceptance_by_kind）。
  それらを適用済みの DB を V16 へ戻す場合は、先にその分を戻してから本手順を実行する。
