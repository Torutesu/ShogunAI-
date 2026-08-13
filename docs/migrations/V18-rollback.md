# V18__traceability_batch_relay.sql — ロールバック手順

対象: `traceability_log.route` の CHECK に `'batch_relay'` を追加
（`docs/batch-relay-design.md` §3.3 / Plan C-2）。

## 影響範囲

non-additive。V14 と同型のテーブル再作成＋全行コピー。列の削除・改名・型変更なし。
`idx_traceability_ts` を張り直す。外部キー参照なし。

`batch_relay` に `third_party` は付けない — 中継は運営自身のインフラであり、
チャンク本文を保存しない（NFR-PRV-04）。Composio / ASR の第三者バッジとは別の開示。

## ロールバック

```sql
BEGIN;

DELETE FROM traceability_log WHERE route = 'batch_relay';

CREATE TABLE traceability_log_v17 (
    id          INTEGER PRIMARY KEY,
    ts          INTEGER NOT NULL,
    route       TEXT    NOT NULL CHECK (route IN ('batch_api', 'messages_api', 'mcp', 'composio', 'billing', 'asr', 'local_agent')),
    purpose     TEXT    NOT NULL,
    destination TEXT    NOT NULL,
    chunk_bytes INTEGER NOT NULL,
    chunk_xxh64 TEXT    NOT NULL,
    third_party INTEGER NOT NULL DEFAULT 0
) STRICT;

INSERT INTO traceability_log_v17 (id, ts, route, purpose, destination, chunk_bytes, chunk_xxh64, third_party)
    SELECT id, ts, route, purpose, destination, chunk_bytes, chunk_xxh64, third_party
    FROM traceability_log;

DROP TABLE traceability_log;
ALTER TABLE traceability_log_v17 RENAME TO traceability_log;
CREATE INDEX idx_traceability_ts ON traceability_log (ts);

DELETE FROM refinery_schema_history WHERE version = 18;

COMMIT;
```

> SQL ヘッダのインライン手順3は当初 `version = 17` を消す指示になっていたが、それでは
> V17 が再実行されて `open()` が壊れる。**消すのは `version = 18`。** ヘッダ側は修正済み。

## データ損失

`route = 'batch_relay'` のトレース行のみ。削除前にエクスポートして保全すること
（AR-11 の監査記録）。チャンク本文は元々保存していないので、消えるのは
「いつ・どこへ・どれだけ」の記録。

## 注意

- CHECK 拡張の連鎖は V13 → V14 → V18。**戻すときは必ず降順**（V18 → V14 → V13）。
  後段の route 値が残ったまま前段の CHECK へ縮めると INSERT が失敗する。
- ロールバック後は Batch レーンを `batch_api`（Anthropic 直・開発時のみ）へ戻すこと。
  出荷版の relay 経路（`RelayBatchClient`）は route を書けなくなると egress を
  記録できない ＝ 不変条件3違反になるため、relay を無効化してから戻す。
