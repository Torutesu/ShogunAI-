-- Issue #63: 圧縮の計測（raw vs compressed 比較）。本文・キャプチャ内容は保存しない。
CREATE TABLE compression_metrics (
    id           INTEGER PRIMARY KEY,
    ts           INTEGER NOT NULL,
    query_hash   TEXT    NOT NULL,                 -- クエリの xxh64（本文は保存しない）
    path         TEXT    NOT NULL CHECK(path IN ('raw','compressed')),
    pre_tokens   INTEGER NOT NULL,
    post_tokens  INTEGER NOT NULL,
    compress_ms  INTEGER NOT NULL,
    assemble_ms  INTEGER NOT NULL
);

CREATE INDEX idx_compression_metrics_ts ON compression_metrics(ts);
