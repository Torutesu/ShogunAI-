-- Batch relay (docs/batch-relay-design.md §3.3, Plan C-2): add 'batch_relay' to the
-- traceability_log.route CHECK.
--
-- 出荷版の Batch レーンはチャンクを Select 運営の中継 (relay.shogun.app) 経由で送る。経路が
-- 変わるので 'batch_api'（Anthropic 直・開発時のみ）と区別できる値を持たせ、トレーサビリティ
-- 画面が「運営サーバー経由」を明示できるようにする（不変条件3）。third_party は付けない ——
-- 中継は運営自身のインフラであり、チャンク本文を保存しない（NFR-PRV-04）。Composio/ASR の
-- 第三者バッジとは別の開示である。
--
-- non-additive-ok: SQLite は CHECK 制約を ALTER できないため、テーブル再作成 + コピー
-- （データ保全リビルド。列の削除・改名・型変更なし）。traceability_log を参照する外部キーは
-- 存在しないので FK 無効化は不要。DROP TABLE は付随インデックスも落とすため
-- idx_traceability_ts を張り直す（V14 と同型）。
--
-- ロールバック手順:
--   1. 下と同じ手順で CHECK から 'batch_relay' を除いたテーブルを作り直す
--   2. コピー前に  DELETE FROM traceability_log WHERE route = 'batch_relay';
--      （トレース行は監査記録であり、消すのは CHECK に収まらない行だけに限ること）
--   3. refinery_schema_history から version = 17 の行を削除する
-- 破壊的な列削除・改名は行っていないため、V16 のコードは V17 のスキーマ上でも動作する。

CREATE TABLE traceability_log_v17 (
    id          INTEGER PRIMARY KEY,
    ts          INTEGER NOT NULL,
    route       TEXT    NOT NULL CHECK (route IN ('batch_api', 'batch_relay', 'messages_api', 'mcp', 'composio', 'billing', 'asr', 'local_agent')),
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
