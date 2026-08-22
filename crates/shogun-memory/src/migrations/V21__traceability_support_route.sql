-- CS / bug-report intake (support窓口): add 'support' to the traceability_log.route CHECK.
--
-- Help & Support から送るレポートは、ユーザーが自分で書いた本文をユーザー自身の操作で運営
-- サーバー（syogun.com）へ送る egress。不変条件3のとおり送信箇所には台帳行を残すが、
-- 'billing'（ライセンス手続き・内容なし）とは開示が違う——本文はユーザー著作のテキストであり、
-- 台帳は「いつ・どこへ・何バイト」を示す（本文は digest のみ、保存しない）。third_party は
-- 付けない: 送信先は運営自身のインフラで、第三者中継を挟まない。
--
-- non-additive-ok: SQLite は CHECK 制約を ALTER できないため、テーブル再作成 + コピー
-- （データ保全リビルド。列の削除・改名・型変更なし）。traceability_log を参照する外部キーは
-- 存在しないので FK 無効化は不要。DROP TABLE は付随インデックスも落とすため
-- idx_traceability_ts を張り直す（V18 と同型）。
--
-- ロールバック手順:
--   1. 下と同じ手順で CHECK から 'support' を除いたテーブルを作り直す
--   2. コピー前に  DELETE FROM traceability_log WHERE route = 'support';
--      （トレース行は監査記録であり、消すのは CHECK に収まらない行だけに限ること）
--   3. refinery_schema_history から version = 21 の行を削除する
-- 破壊的な列削除・改名は行っていないため、V20 のコードは V21 のスキーマ上でも動作する。

CREATE TABLE traceability_log_v21 (
    id          INTEGER PRIMARY KEY,
    ts          INTEGER NOT NULL,
    route       TEXT    NOT NULL CHECK (route IN ('batch_api', 'batch_relay', 'messages_api', 'mcp', 'composio', 'billing', 'asr', 'local_agent', 'support')),
    purpose     TEXT    NOT NULL,
    destination TEXT    NOT NULL,
    chunk_bytes INTEGER NOT NULL,
    chunk_xxh64 TEXT    NOT NULL,
    third_party INTEGER NOT NULL DEFAULT 0
) STRICT;

INSERT INTO traceability_log_v21 (id, ts, route, purpose, destination, chunk_bytes, chunk_xxh64, third_party)
    SELECT id, ts, route, purpose, destination, chunk_bytes, chunk_xxh64, third_party
    FROM traceability_log;

DROP TABLE traceability_log;

ALTER TABLE traceability_log_v21 RENAME TO traceability_log;

CREATE INDEX idx_traceability_ts ON traceability_log (ts);
