# V21__traceability_support_route.sql — ロールバック手順

対象: `traceability_log.route` の CHECK に `'support'` を追加
（CS / バグ報告窓口。Help & Support からの送信を台帳に記録する）。

## 影響範囲

non-additive。V18 と同型のテーブル再作成＋全行コピー。列の削除・改名・型変更なし。
`idx_traceability_ts` を張り直す。外部キー参照なし。

`support` に `third_party` は付けない — 送信先は運営自身のインフラ（syogun.com の
support intake）であり、第三者中継を挟まない。本文はユーザーが自分で書いた
レポートで、台帳にはバイト数と digest のみ残る（本文は保存しない。G8）。

## ロールバック

```sql
BEGIN;

DELETE FROM traceability_log WHERE route = 'support';

CREATE TABLE traceability_log_v20 (
    id          INTEGER PRIMARY KEY,
    ts          INTEGER NOT NULL,
    route       TEXT    NOT NULL CHECK (route IN ('batch_api', 'batch_relay', 'messages_api', 'mcp', 'composio', 'billing', 'asr', 'local_agent')),
    purpose     TEXT    NOT NULL,
    destination TEXT    NOT NULL,
    chunk_bytes INTEGER NOT NULL,
    chunk_xxh64 TEXT    NOT NULL,
    third_party INTEGER NOT NULL DEFAULT 0
) STRICT;

INSERT INTO traceability_log_v20 (id, ts, route, purpose, destination, chunk_bytes, chunk_xxh64, third_party)
    SELECT id, ts, route, purpose, destination, chunk_bytes, chunk_xxh64, third_party
    FROM traceability_log;

DROP TABLE traceability_log;
ALTER TABLE traceability_log_v20 RENAME TO traceability_log;
CREATE INDEX idx_traceability_ts ON traceability_log (ts);

DELETE FROM refinery_schema_history WHERE version = 21;

COMMIT;
```

## データ損失

`route = 'support'` のトレース行のみ。削除前にエクスポートして保全すること
（AR-11 の監査記録）。チャンク本文は元々保存していないので、消えるのは
「いつ・どこへ・どれだけ」の記録。

## 注意

- CHECK 拡張の連鎖は V13 → V14 → V18 → V21。**戻すときは必ず降順**
  （V21 → V18 → V14 → V13）。後段の route 値が残ったまま前段の CHECK へ縮めると
  INSERT が失敗する。
- ロールバック後は Help & Support の送信を無効化すること。route を書けないまま
  送信を残すと egress が記録できない ＝ 不変条件3違反になる。
  `LATEST_SCHEMA_VERSION`（crates/shogun-memory/src/lib.rs）も 20 へ戻す。
