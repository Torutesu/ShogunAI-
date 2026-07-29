//! 圧縮の計測（Issue #63）。raw / compressed の各パスの前後トークン数と処理時間を記録し、
//! AB 比較に使う。**本文・キャプチャ内容は保存しない**（テレメトリ規約 G8）。クエリは
//! `query_hash`（呼び出し側で xxh64 済み）のみ。

use rusqlite::{params, Connection};

/// 1 回の組み立てで記録する計測行。
#[derive(Debug, Clone, PartialEq)]
pub struct MetricRow {
    pub ts: i64,
    pub query_hash: String,
    /// "raw" または "compressed"。
    pub path: String,
    pub pre_tokens: i64,
    pub post_tokens: i64,
    pub compress_ms: i64,
    pub assemble_ms: i64,
}

/// 1 行を挿入する。best-effort（呼び出し側が失敗を無視できるよう Result を返す）。
pub fn insert(conn: &Connection, row: &MetricRow) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO compression_metrics
           (ts, query_hash, path, pre_tokens, post_tokens, compress_ms, assemble_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            row.ts,
            row.query_hash,
            row.path,
            row.pre_tokens,
            row.post_tokens,
            row.compress_ms,
            row.assemble_ms
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// あるパスの平均削減率（1 - post/pre）を返す。行が無ければ None。
pub fn avg_reduction(conn: &Connection, path: &str) -> rusqlite::Result<Option<f64>> {
    let v: Option<f64> = conn.query_row(
        "SELECT AVG(1.0 - CAST(post_tokens AS REAL) / NULLIF(pre_tokens, 0))
           FROM compression_metrics WHERE path = ?1",
        params![path],
        |r| r.get(0),
    )?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE compression_metrics (
                id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, query_hash TEXT NOT NULL,
                path TEXT NOT NULL CHECK(path IN ('raw','compressed')),
                pre_tokens INTEGER NOT NULL, post_tokens INTEGER NOT NULL,
                compress_ms INTEGER NOT NULL, assemble_ms INTEGER NOT NULL);",
        )
        .unwrap();
        c
    }

    fn row(path: &str, pre: i64, post: i64) -> MetricRow {
        MetricRow {
            ts: 1_000,
            query_hash: "deadbeef".into(),
            path: path.into(),
            pre_tokens: pre,
            post_tokens: post,
            compress_ms: 5,
            assemble_ms: 20,
        }
    }

    #[test]
    fn insert_returns_rowid() {
        let c = conn();
        let id = insert(&c, &row("compressed", 100, 30)).unwrap();
        assert_eq!(id, 1);
    }

    #[test]
    fn avg_reduction_computes_ratio() {
        let c = conn();
        insert(&c, &row("compressed", 100, 20)).unwrap(); // 0.8
        insert(&c, &row("compressed", 100, 40)).unwrap(); // 0.6
        let avg = avg_reduction(&c, "compressed").unwrap().unwrap();
        assert!((avg - 0.7).abs() < 1e-9, "avg={avg}");
    }

    #[test]
    fn avg_reduction_none_when_empty() {
        let c = conn();
        assert_eq!(avg_reduction(&c, "compressed").unwrap(), None);
    }
}
