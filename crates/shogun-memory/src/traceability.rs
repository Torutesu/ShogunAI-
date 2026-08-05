//! Traceability-log repository (§6.14, FR-TR-01/04). The storage side of the traceability record:
//! shogun-memory owns the `traceability_log` table (invariant 1 — data gravity in the Rust core),
//! so the write/read of trace rows lives here, not in the caller.
//!
//! The table (V1__init.sql) stores **only** the chunk's byte length + an xxh64 digest, never the
//! sent text (privacy rule / G8). There is no text column, so a body cannot be persisted here even
//! by mistake. `route` is constrained by a CHECK to the known route set; [`Route`] mirrors that
//! set exactly, so an insert always produces a valid value and a read always parses back.
//!
//! The LLM layer's `TraceRecord` (shogun-core) maps 1:1 onto [`TraceRow`]; the daemon bridges the
//! two when it wires the sink. Keeping the enum here (rather than depending on shogun-core) keeps
//! the storage layer free of an upward dependency.

use rusqlite::{params, Connection};

/// A send route, mirroring the `traceability_log.route` CHECK set exactly (FR-TR-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    BatchApi,
    MessagesApi,
    Mcp,
    Composio,
    Billing,
    /// Agent inference delegated to a local, already-signed-in vendor CLI running on the user's own
    /// subscription (Issue #110). Distinct from [`Route::MessagesApi`]: SHOGUN holds no credential
    /// and does not open the socket — a separate local process does, against the user's plan quota.
    LocalAgent,
}

impl Route {
    /// The exact string stored in the column.
    pub fn as_str(self) -> &'static str {
        match self {
            Route::BatchApi => "batch_api",
            Route::MessagesApi => "messages_api",
            Route::Mcp => "mcp",
            Route::Composio => "composio",
            Route::Billing => "billing",
            Route::LocalAgent => "local_agent",
        }
    }

    /// Parse the stored string back. `None` for an unknown value (shouldn't occur — the CHECK
    /// prevents writing one — but reads stay total).
    pub fn parse(s: &str) -> Option<Route> {
        Some(match s {
            "batch_api" => Route::BatchApi,
            "messages_api" => Route::MessagesApi,
            "mcp" => Route::Mcp,
            "composio" => Route::Composio,
            "billing" => Route::Billing,
            "local_agent" => Route::LocalAgent,
            _ => return None,
        })
    }

    /// Composio is the only third-party route (FR-C2-04); used to default the badge.
    pub fn is_third_party(self) -> bool {
        matches!(self, Route::Composio)
    }
}

/// A traceability row: byte length + digest of an outbound chunk, never its text (G8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceRow {
    pub ts: i64,
    pub route: Route,
    pub purpose: String,
    pub destination: String,
    pub chunk_bytes: i64,
    /// Lower-hex xxh64 of the chunk (a digest, never the text).
    pub chunk_xxh64: String,
    pub third_party: bool,
}

impl TraceRow {
    /// Build a row for a chunk. `third_party` defaults to the route (Composio ⇒ true) but the
    /// caller may still be explicit via the struct.
    pub fn new(
        ts: i64,
        route: Route,
        purpose: impl Into<String>,
        destination: impl Into<String>,
        chunk_bytes: i64,
        chunk_xxh64: impl Into<String>,
    ) -> Self {
        Self {
            ts,
            route,
            purpose: purpose.into(),
            destination: destination.into(),
            chunk_bytes,
            chunk_xxh64: chunk_xxh64.into(),
            third_party: route.is_third_party(),
        }
    }
}

/// Append a traceability row (one per external send, AR-11). Returns the new row id.
pub fn insert(conn: &Connection, row: &TraceRow) -> Result<i64, rusqlite::Error> {
    conn.execute(
        "INSERT INTO traceability_log
           (ts, route, purpose, destination, chunk_bytes, chunk_xxh64, third_party)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            row.ts,
            row.route.as_str(),
            row.purpose,
            row.destination,
            row.chunk_bytes,
            row.chunk_xxh64,
            row.third_party as i64,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Count traceability rows written at or after `ts` — the number of outbound chunks a Dream Cycle
/// sent during a run (FR-DC-06), given the run's start time.
pub fn count_since(conn: &Connection, ts: i64) -> Result<i64, rusqlite::Error> {
    conn.query_row("SELECT count(*) FROM traceability_log WHERE ts >= ?1", params![ts], |r| r.get(0))
}

/// A read-back filter for the viewer (FR-TR-02). `None` fields don't constrain; `destination` is a
/// case-insensitive substring (SQL `LIKE`).
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub route: Option<Route>,
    pub purpose: Option<String>,
    pub destination: Option<String>,
}

/// List rows matching `filter`, most-recent first (FR-TR-02 時系列一覧).
pub fn list(conn: &Connection, filter: &Filter) -> Result<Vec<TraceRow>, rusqlite::Error> {
    // Build the WHERE incrementally with bound params (never string-interpolate user input).
    let mut sql = String::from(
        "SELECT ts, route, purpose, destination, chunk_bytes, chunk_xxh64, third_party
         FROM traceability_log",
    );
    let mut clauses: Vec<String> = Vec::new();
    let mut binds: Vec<rusqlite::types::Value> = Vec::new();

    if let Some(r) = filter.route {
        clauses.push(format!("route = ?{}", binds.len() + 1));
        binds.push(r.as_str().to_string().into());
    }
    if let Some(p) = &filter.purpose {
        clauses.push(format!("purpose = ?{}", binds.len() + 1));
        binds.push(p.clone().into());
    }
    if let Some(d) = &filter.destination {
        clauses.push(format!("destination LIKE ?{}", binds.len() + 1));
        binds.push(format!("%{d}%").into());
    }
    if !clauses.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&clauses.join(" AND "));
    }
    sql.push_str(" ORDER BY ts DESC, id DESC");

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(binds.iter()), |r| {
        let route_str: String = r.get(1)?;
        Ok(TraceRow {
            ts: r.get(0)?,
            route: Route::parse(&route_str).unwrap_or(Route::BatchApi),
            purpose: r.get(2)?,
            destination: r.get(3)?,
            chunk_bytes: r.get(4)?,
            chunk_xxh64: r.get(5)?,
            third_party: r.get::<_, i64>(6)? != 0,
        })
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(ts: i64, route: Route, purpose: &str, dest: &str) -> TraceRow {
        TraceRow::new(ts, route, purpose, dest, 128, "0123456789abcdef")
    }

    #[test]
    fn insert_returns_incrementing_ids_and_persists_digest_only() {
        let conn = crate::open_in_memory().unwrap();
        let a = insert(&conn, &row(100, Route::BatchApi, "dream_cycle", "api.anthropic.com")).unwrap();
        let b = insert(&conn, &row(200, Route::MessagesApi, "agent", "api.anthropic.com")).unwrap();
        assert!(b > a);
        // the table has no text column — assert only digest+len are stored
        let (bytes, digest): (i64, String) = conn
            .query_row("SELECT chunk_bytes, chunk_xxh64 FROM traceability_log WHERE id = ?1", [a], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(bytes, 128);
        assert_eq!(digest, "0123456789abcdef");
    }

    #[test]
    fn composio_row_defaults_third_party_true() {
        let conn = crate::open_in_memory().unwrap();
        insert(&conn, &row(1, Route::Composio, "agent", "gmail.com")).unwrap();
        let tp: i64 = conn.query_row("SELECT third_party FROM traceability_log", [], |r| r.get(0)).unwrap();
        assert_eq!(tp, 1);
    }

    #[test]
    fn check_constraint_rejects_an_unknown_route_string() {
        // Route::as_str only produces valid values; a raw bad string must be refused by the CHECK.
        let conn = crate::open_in_memory().unwrap();
        let err = conn.execute(
            "INSERT INTO traceability_log (ts, route, purpose, destination, chunk_bytes, chunk_xxh64)
             VALUES (1, 'sneaky', 'p', 'd', 1, 'h')",
            [],
        );
        assert!(err.is_err(), "CHECK must reject an unknown route");
    }

    #[test]
    fn local_agent_route_is_accepted_and_is_not_third_party() {
        // V12 widened the CHECK. A subscription-delegated send must be storable, and must NOT get
        // the third-party badge: the user's own vendor on the user's own plan is not a relay
        // (unlike Composio, FR-C2-04).
        let conn = crate::open_in_memory().unwrap();
        insert(&conn, &row(1, Route::LocalAgent, "agent", "api.anthropic.com")).unwrap();
        let (route, tp): (String, i64) = conn
            .query_row("SELECT route, third_party FROM traceability_log", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(route, "local_agent");
        assert_eq!(tp, 0);
    }

    #[test]
    fn list_is_most_recent_first() {
        let conn = crate::open_in_memory().unwrap();
        insert(&conn, &row(100, Route::BatchApi, "indexing", "api.anthropic.com")).unwrap();
        insert(&conn, &row(300, Route::Composio, "agent", "gmail.com")).unwrap();
        insert(&conn, &row(200, Route::MessagesApi, "chat", "api.anthropic.com")).unwrap();
        let all = list(&conn, &Filter::default()).unwrap();
        let ts: Vec<i64> = all.iter().map(|r| r.ts).collect();
        assert_eq!(ts, vec![300, 200, 100]);
    }

    #[test]
    fn list_filters_by_route_and_destination() {
        let conn = crate::open_in_memory().unwrap();
        insert(&conn, &row(1, Route::BatchApi, "indexing", "api.anthropic.com")).unwrap();
        insert(&conn, &row(2, Route::Composio, "agent", "gmail.com")).unwrap();
        insert(&conn, &row(3, Route::MessagesApi, "chat", "api.anthropic.com")).unwrap();

        let composio = list(&conn, &Filter { route: Some(Route::Composio), ..Default::default() }).unwrap();
        assert_eq!(composio.len(), 1);
        assert!(composio[0].third_party);

        let anthropic = list(
            &conn,
            &Filter { destination: Some("ANTHROPIC".into()), ..Default::default() },
        )
        .unwrap();
        assert_eq!(anthropic.len(), 2); // case-insensitive substring
    }

    #[test]
    fn list_filters_by_purpose() {
        let conn = crate::open_in_memory().unwrap();
        insert(&conn, &row(1, Route::BatchApi, "dream_cycle", "x")).unwrap();
        insert(&conn, &row(2, Route::BatchApi, "morning_brief", "x")).unwrap();
        let mb = list(&conn, &Filter { purpose: Some("morning_brief".into()), ..Default::default() }).unwrap();
        assert_eq!(mb.len(), 1);
        assert_eq!(mb[0].purpose, "morning_brief");
    }

    #[test]
    fn route_string_roundtrips() {
        for r in [
            Route::BatchApi,
            Route::MessagesApi,
            Route::Mcp,
            Route::Composio,
            Route::Billing,
            Route::LocalAgent,
        ] {
            assert_eq!(Route::parse(r.as_str()), Some(r));
        }
        assert_eq!(Route::parse("nope"), None);
    }
}
