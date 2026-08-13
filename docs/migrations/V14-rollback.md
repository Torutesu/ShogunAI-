# V14__traceability_local_agent.sql — ロールバック手順

対象: `traceability_log.route` の CHECK に `'local_agent'` を追加（Issue #110、サブスク委譲で
ローカルの公式 CLI に推論を委ねる経路）。

## 影響範囲

non-additive。V13 と同型のテーブル再作成＋全行コピー。列の削除・改名・型変更なし。
`idx_traceability_ts` を張り直す。外部キー参照なし。

なお本マイグレーションは当初 V12 として書かれたが、`screen_frames`（V12）/ `asr`（V13）と
並行開発で採番が衝突したためマージ時に V14 へ改番している。CHECK は V13 の集合に
`'local_agent'` を足した形。

## ロールバック

```sql
BEGIN;

DELETE FROM traceability_log WHERE route = 'local_agent';

CREATE TABLE traceability_log_v13 (
    id          INTEGER PRIMARY KEY,
    ts          INTEGER NOT NULL,
    route       TEXT    NOT NULL CHECK (route IN ('batch_api', 'messages_api', 'mcp', 'composio', 'billing', 'asr')),
    purpose     TEXT    NOT NULL,
    destination TEXT    NOT NULL,
    chunk_bytes INTEGER NOT NULL,
    chunk_xxh64 TEXT    NOT NULL,
    third_party INTEGER NOT NULL DEFAULT 0
) STRICT;

INSERT INTO traceability_log_v13 (id, ts, route, purpose, destination, chunk_bytes, chunk_xxh64, third_party)
    SELECT id, ts, route, purpose, destination, chunk_bytes, chunk_xxh64, third_party
    FROM traceability_log;

DROP TABLE traceability_log;
ALTER TABLE traceability_log_v13 RENAME TO traceability_log;
CREATE INDEX idx_traceability_ts ON traceability_log (ts);

DELETE FROM refinery_schema_history WHERE version = 14;

COMMIT;
```

## データ損失

`route = 'local_agent'` のトレース行のみ。V13 と同様、削除前にエクスポートして保全すること。

## 注意

- V18 が同じ CHECK をさらに広げている。**V18 を先に戻してから V14 を戻すこと。**
- ロールバックすると委譲経路の egress を記録できなくなる。委譲は BYOK 経路
  （`messages_api`）と開示内容が異なる（SHOGUN が資格情報を持たず、ユーザーのサブスク枠で
  ローカルの別プロセスが送信する）ため、FR-TR-01/02 の表示要件を満たすには
  **Agent lane のサブスク委譲そのものを無効化してから**戻す。route を書けないまま
  委譲を動かすと、記録されない egress が発生する（不変条件3違反）。
