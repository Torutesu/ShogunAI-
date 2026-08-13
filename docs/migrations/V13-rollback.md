# V13__traceability_asr_route.sql — ロールバック手順

対象: `traceability_log.route` の CHECK に `'asr'` を追加（会議 Deepgram STT、2026-08-05 の例外決定）。

## 影響範囲

non-additive。SQLite は CHECK を ALTER できないため、テーブル再作成＋全行コピー
（create-copy-drop-rename）。列の削除・改名・型変更はなく、`idx_traceability_ts` を張り直す。
`traceability_log` を参照する外部キーは存在しないので FK 無効化は不要。

## ロールバック

```sql
BEGIN;

-- 1) CHECK に収まらない行だけを落とす。トレース行は監査記録なので、
--    これ以外の行を消してはならない。
DELETE FROM traceability_log WHERE route = 'asr';

-- 2) V12 時点の CHECK でテーブルを作り直し、全行をコピーする
CREATE TABLE traceability_log_v12 (
    id          INTEGER PRIMARY KEY,
    ts          INTEGER NOT NULL,
    route       TEXT    NOT NULL CHECK (route IN ('batch_api', 'messages_api', 'mcp', 'composio', 'billing')),
    purpose     TEXT    NOT NULL,
    destination TEXT    NOT NULL,
    chunk_bytes INTEGER NOT NULL,
    chunk_xxh64 TEXT    NOT NULL,
    third_party INTEGER NOT NULL DEFAULT 0
) STRICT;

INSERT INTO traceability_log_v12 (id, ts, route, purpose, destination, chunk_bytes, chunk_xxh64, third_party)
    SELECT id, ts, route, purpose, destination, chunk_bytes, chunk_xxh64, third_party
    FROM traceability_log;

DROP TABLE traceability_log;
ALTER TABLE traceability_log_v12 RENAME TO traceability_log;
CREATE INDEX idx_traceability_ts ON traceability_log (ts);

DELETE FROM refinery_schema_history WHERE version = 13;

COMMIT;
```

## データ損失

`route = 'asr'` のトレース行が消える。これは AR-11 の監査記録なので、**削除前に
`SELECT * FROM traceability_log WHERE route = 'asr'` をエクスポートして保全すること。**
チャンク本文は元々保存していない（`chunk_xxh64` とバイト数のみ）ため、消えるのは
「いつ・どこへ・どれだけ送ったか」の記録であって内容ではない。

## 注意

V14 / V18 が同じ CHECK を後から広げている。**V13 単体へ戻す前に V18 → V14 の順で
戻すこと**（後段の route 値を残したまま V12 の CHECK へ縮めると INSERT が失敗する）。
ロールバック後は Deepgram ライブ STT 経路（`voice_lane` の `TraceabilitySink`）も
V13 以前へ戻す必要がある — sink は必須依存なので、route が書けなくなると
文字起こしが hard-stop する。
