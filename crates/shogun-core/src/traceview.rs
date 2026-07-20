//! Traceability viewer model (§6.14, FR-TR-01/02/04). The write path records a digest at send time
//! ([`crate::llm::traceability`]); this is the **read/display** model the Full UI viewer renders
//! from the persisted `traceability_log`: the full FR-TR-01 field set, chronological + filtered
//! views, the third-party badge, and the 7-day full-text window.
//!
//! Purely a projection/filter over rows — no I/O — so the viewer rules (what filters match, when
//! full text is still available, which entries carry the third-party badge) are Linux-testable.

/// The route a send took (FR-TR-01 `route`). Composio entries get the third-party badge (FR-C2-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceRoute {
    Direct,
    ViaComposio,
}

impl TraceRoute {
    /// FR-TR-02 / FR-C2-04: Composio-routed entries are shown "third-party".
    pub fn is_third_party(self) -> bool {
        matches!(self, TraceRoute::ViaComposio)
    }
}

/// The purpose enum (FR-TR-01 `purpose`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    DreamCycle,
    MorningBrief,
    Indexing,
    Agent,
    Chat,
    IntegrationWrite,
    IntegrationRead,
}

/// The key kind used for the send (FR-TR-01 `key_kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    SelectKk,
    Byok,
    OauthUser,
    Composio,
}

/// The approval level under which the send happened (FR-TR-01 `approval`). `Background` covers
/// non-action sends such as sync reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approval {
    L1,
    L2,
    L3,
    Background,
}

/// The send's outcome (FR-TR-01 `status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Success,
    Failure,
}

/// The payload digest (FR-TR-01 `payload_digest`): a head excerpt + size + chunk count. The full
/// text is kept locally for 7 days only (see [`full_text_available`]); the digest is kept forever.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadDigest {
    /// A short head excerpt for the list view (never the full body).
    pub excerpt: String,
    pub size_bytes: usize,
    pub chunk_count: u32,
}

/// One traceability entry as shown in the viewer (FR-TR-01 fields).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceEntry {
    pub id: i64,
    pub ts: i64,
    pub destination: String,
    pub route: TraceRoute,
    pub purpose: Purpose,
    pub key_kind: KeyKind,
    pub approval: Approval,
    pub digest: PayloadDigest,
    pub status: Status,
}

impl TraceEntry {
    /// Whether this entry should show the "third-party (via Composio)" badge.
    pub fn is_third_party(&self) -> bool {
        self.route.is_third_party()
    }
}

/// Full-text local retention: 7 days (FR-TR-04). After this only the digest remains.
pub const FULL_TEXT_RETENTION_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// Whether an entry's full sent text is still available (within the 7-day window, FR-TR-02/04).
pub fn full_text_available(entry_ts: i64, now_ms: i64) -> bool {
    now_ms.saturating_sub(entry_ts) <= FULL_TEXT_RETENTION_MS
}

/// A viewer filter (FR-TR-02): by destination / purpose / route / key kind. `None` fields don't
/// constrain. Destination matches as a case-insensitive substring.
#[derive(Debug, Clone, Default)]
pub struct TraceFilter {
    pub destination: Option<String>,
    pub purpose: Option<Purpose>,
    pub route: Option<TraceRoute>,
    pub key_kind: Option<KeyKind>,
}

impl TraceFilter {
    /// Whether an entry passes this filter.
    pub fn matches(&self, e: &TraceEntry) -> bool {
        if let Some(d) = &self.destination {
            if !e.destination.to_lowercase().contains(&d.to_lowercase()) {
                return false;
            }
        }
        if let Some(p) = self.purpose {
            if e.purpose != p {
                return false;
            }
        }
        if let Some(r) = self.route {
            if e.route != r {
                return false;
            }
        }
        if let Some(k) = self.key_kind {
            if e.key_kind != k {
                return false;
            }
        }
        true
    }
}

/// Build the viewer list: entries passing `filter`, most-recent first (FR-TR-02 時系列一覧).
pub fn view<'a>(entries: &'a [TraceEntry], filter: &TraceFilter) -> Vec<&'a TraceEntry> {
    let mut out: Vec<&TraceEntry> = entries.iter().filter(|e| filter.matches(e)).collect();
    out.sort_by(|a, b| b.ts.cmp(&a.ts));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: i64, ts: i64, dest: &str, route: TraceRoute, purpose: Purpose, key: KeyKind) -> TraceEntry {
        TraceEntry {
            id,
            ts,
            destination: dest.into(),
            route,
            purpose,
            key_kind: key,
            approval: Approval::L3,
            digest: PayloadDigest { excerpt: "Subject: …".into(), size_bytes: 128, chunk_count: 1 },
            status: Status::Success,
        }
    }

    fn sample() -> Vec<TraceEntry> {
        vec![
            entry(1, 100, "api.anthropic.com", TraceRoute::Direct, Purpose::DreamCycle, KeyKind::SelectKk),
            entry(2, 300, "gmail.com", TraceRoute::ViaComposio, Purpose::Agent, KeyKind::Composio),
            entry(3, 200, "api.anthropic.com", TraceRoute::Direct, Purpose::Chat, KeyKind::Byok),
        ]
    }

    #[test]
    fn view_is_most_recent_first() {
        let all = sample();
        let v = view(&all, &TraceFilter::default());
        let ids: Vec<i64> = v.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![2, 3, 1]); // ts 300, 200, 100
    }

    #[test]
    fn filter_by_purpose_and_key_kind() {
        let all = sample();
        let f = TraceFilter { purpose: Some(Purpose::Chat), ..Default::default() };
        let v = view(&all, &f);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].id, 3);

        let f2 = TraceFilter { key_kind: Some(KeyKind::SelectKk), ..Default::default() };
        assert_eq!(view(&all, &f2).len(), 1);
    }

    #[test]
    fn filter_by_destination_substring_case_insensitive() {
        let all = sample();
        let f = TraceFilter { destination: Some("ANTHROPIC".into()), ..Default::default() };
        assert_eq!(view(&all, &f).len(), 2);
    }

    #[test]
    fn filter_by_route() {
        let all = sample();
        let f = TraceFilter { route: Some(TraceRoute::ViaComposio), ..Default::default() };
        let v = view(&all, &f);
        assert_eq!(v.len(), 1);
        assert!(v[0].is_third_party());
    }

    #[test]
    fn composio_entries_carry_third_party_badge() {
        let all = sample();
        let composio = all.iter().find(|e| e.id == 2).unwrap();
        let direct = all.iter().find(|e| e.id == 1).unwrap();
        assert!(composio.is_third_party());
        assert!(!direct.is_third_party());
    }

    #[test]
    fn full_text_available_only_within_seven_days() {
        let ts = 1_000_000;
        assert!(full_text_available(ts, ts)); // same instant
        assert!(full_text_available(ts, ts + FULL_TEXT_RETENTION_MS)); // exactly 7 days
        assert!(!full_text_available(ts, ts + FULL_TEXT_RETENTION_MS + 1)); // just past
    }
}
