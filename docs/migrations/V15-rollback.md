# V15__briefs.sql — ロールバック手順

対象: `briefs`（永続化された Morning Brief。Plan C-1 / §6.8 FR-MB-01..06）。

## 影響範囲

additive のみ。新規テーブル1つ、索引なし、外部キー参照なし。
1ローカル日 = 1行で、夜間の Dream Cycle が UPSERT し、朝は読むだけ。

## ロールバック

```sql
BEGIN;
DROP TABLE IF EXISTS briefs;
DELETE FROM refinery_schema_history WHERE version = 15;
COMMIT;
```

## データ損失

生成済みの Morning Brief（`payload` JSON）が全日分消える。ただし brief は
`event_log` / state tables / `commitments` から**再構成できる派生物**であり、原本ではない。
V15 以前のコードは「その場で組み立てる」経路を持っているので、機能自体は劣化しつつも動く。

失われて再現できないのは `prev_digest` に基づく FR-MB-06 の "Updated" マークだけ
（前回表示との差分は履歴を持たないと出せない）。ロールバック後の初回表示では
Updated マークが付かなくなる。

## 注意

- 削除前に `SELECT date, payload FROM briefs` を退避しておけば、V15 へ戻したときに
  そのまま INSERT で復元できる（スキーマは単純な key-value）。
- `generated` 列は「モデル生成の文章を添えたか（1）／抽出のみの正直な劣化か（0）」の記録。
  FR-MB-04 の劣化表示監査に使うため、退避対象に含めること。
