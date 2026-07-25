//! Cross-channel identity resolution — 名寄せ (FR-ST-10). The pure decision at the heart of merging
//! one person who appears as a Gmail address, a Slack handle, and a GitHub login into a single
//! `people` row. The Dream Cycle runs this over incoming identities; the DB merge (and the user's
//! *split* correction) wrap it.
//!
//! The overriding rule is **do not mis-merge**: a wrong merge fuses two real people and is worse
//! than a missed one (the user can always merge later, but un-fusing is disruptive — hence the
//! split affordance). So only an **exact channel-identity match** (same email, or same handle on the
//! same platform) auto-merges, at high confidence. A name-only overlap is surfaced as a weak
//! *possible* match that never auto-merges — it stays below the merge threshold and is offered for
//! confirmation, mirroring the confidence gate (FR-ST-20) used everywhere else.

/// The channel an identity came from. Two handles only match when their kind matches too — a Slack
/// handle `alice` and a GitHub login `alice` are not the same identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Email,
    Slack,
    GitHub,
    Linear,
    Notion,
}

/// One channel identity observed for a person (an email address or a platform handle).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub channel: Channel,
    /// The raw handle/address. Compared case-insensitively and trimmed.
    pub value: String,
}

impl Identity {
    pub fn new(channel: Channel, value: impl Into<String>) -> Self {
        Self { channel, value: value.into() }
    }

    /// The normalized comparison key (lower-cased, trimmed). Emails compare whole (local@domain).
    fn key(&self) -> String {
        self.value.trim().to_lowercase()
    }

    /// Whether two identities are the *same* channel identity (same channel + same normalized key).
    fn same_as(&self, other: &Identity) -> bool {
        self.channel == other.channel && self.key() == other.key()
    }
}

/// A known person's identity fingerprint, projected from the `people` row (its emails + handles).
/// `display_name` is used only for the weak name-overlap tier, never for an auto-merge.
#[derive(Debug, Clone)]
pub struct PersonIdentities {
    pub person_id: i64,
    pub display_name: String,
    pub identities: Vec<Identity>,
}

/// Confidence assigned to an exact channel-identity match (high — a shared address/handle is strong
/// evidence of the same person). Above the merge threshold.
pub const EXACT_MATCH_CONFIDENCE: f64 = 0.95;

/// Confidence assigned to a name-only overlap (weak — names collide). Below the merge threshold, so
/// it is offered as a *possible* match, never auto-merged.
pub const NAME_ONLY_CONFIDENCE: f64 = 0.35;

/// At/above this confidence a match auto-merges in the Dream Cycle; below it, the match is a
/// suggestion the user confirms. Mirrors the Medium/High confidence bands (FR-ST-20).
pub const MERGE_THRESHOLD: f64 = 0.5;

// Compile-time guarantees: an exact match auto-merges; a name-only overlap never does.
const _: () = assert!(EXACT_MATCH_CONFIDENCE >= MERGE_THRESHOLD);
const _: () = assert!(NAME_ONLY_CONFIDENCE < MERGE_THRESHOLD);

/// The resolution for an incoming identity.
#[derive(Debug, Clone, PartialEq)]
pub enum Resolution {
    /// The identity matches an existing person; carries the person and the merge confidence.
    Match { person_id: i64, confidence: f64 },
    /// A weak name-only overlap — a suggestion, not an auto-merge (confidence below the threshold).
    Possible { person_id: i64, confidence: f64 },
    /// No match — this identity belongs to a new person.
    New,
}

impl Resolution {
    /// Whether this resolution should auto-merge (confidence ≥ [`MERGE_THRESHOLD`]).
    pub fn is_auto_merge(&self) -> bool {
        matches!(self, Resolution::Match { confidence, .. } if *confidence >= MERGE_THRESHOLD)
    }
}

/// Resolve an incoming identity (optionally with the display name it was seen under) against the
/// known people. An exact channel-identity match wins (high confidence); failing that, a name-only
/// overlap is offered as a weak *possible* match; failing that, it's a new person.
///
/// Determinism: the first person (in input order) with an exact match is chosen, so callers get a
/// stable result. Name-only overlap is only considered when no exact match exists.
pub fn resolve(incoming: &Identity, seen_name: Option<&str>, people: &[PersonIdentities]) -> Resolution {
    // 1. Exact channel-identity match — the only auto-merge path.
    if let Some(p) = people.iter().find(|p| p.identities.iter().any(|id| id.same_as(incoming))) {
        return Resolution::Match { person_id: p.person_id, confidence: EXACT_MATCH_CONFIDENCE };
    }
    // 2. Name-only overlap — a weak suggestion, never an auto-merge (names are not unique).
    if let Some(name) = seen_name.map(|n| n.trim().to_lowercase()).filter(|n| !n.is_empty()) {
        if let Some(p) = people.iter().find(|p| p.display_name.trim().to_lowercase() == name) {
            return Resolution::Possible { person_id: p.person_id, confidence: NAME_ONLY_CONFIDENCE };
        }
    }
    Resolution::New
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(id: i64, name: &str, ids: &[(Channel, &str)]) -> PersonIdentities {
        PersonIdentities {
            person_id: id,
            display_name: name.to_string(),
            identities: ids.iter().map(|(c, v)| Identity::new(*c, *v)).collect(),
        }
    }

    #[test]
    fn exact_email_match_auto_merges_at_high_confidence() {
        let people = vec![person(1, "Alice Ng", &[(Channel::Email, "alice@corp.com")])];
        let r = resolve(&Identity::new(Channel::Email, "Alice@Corp.com"), None, &people);
        assert_eq!(r, Resolution::Match { person_id: 1, confidence: EXACT_MATCH_CONFIDENCE });
        assert!(r.is_auto_merge(), "an exact email match auto-merges");
    }

    #[test]
    fn exact_handle_match_is_channel_scoped() {
        // same string, different channel → NOT a match (a Slack `alice` ≠ a GitHub `alice`).
        let people = vec![person(1, "Alice", &[(Channel::Slack, "alice")])];
        let github = resolve(&Identity::new(Channel::GitHub, "alice"), None, &people);
        assert_eq!(github, Resolution::New, "same handle on a different platform is not the same identity");
        let slack = resolve(&Identity::new(Channel::Slack, "alice"), None, &people);
        assert_eq!(slack, Resolution::Match { person_id: 1, confidence: EXACT_MATCH_CONFIDENCE });
    }

    #[test]
    fn name_only_overlap_is_a_weak_possible_never_auto_merge() {
        // A different email but the same display name → possible, not a merge (names collide).
        let people = vec![person(1, "John Smith", &[(Channel::Email, "john@a.com")])];
        let r = resolve(&Identity::new(Channel::Email, "jsmith@b.com"), Some("John Smith"), &people);
        assert_eq!(r, Resolution::Possible { person_id: 1, confidence: NAME_ONLY_CONFIDENCE });
        assert!(!r.is_auto_merge(), "a name-only overlap must never auto-merge (avoid mis-merge)");
    }

    #[test]
    fn exact_match_wins_over_name_overlap() {
        // Two people: one shares the email, another shares the name. Exact identity wins.
        let people = vec![
            person(1, "Someone Else", &[(Channel::Email, "shared@x.com")]),
            person(2, "Jane Doe", &[(Channel::Email, "jane@y.com")]),
        ];
        let r = resolve(&Identity::new(Channel::Email, "shared@x.com"), Some("Jane Doe"), &people);
        assert_eq!(r, Resolution::Match { person_id: 1, confidence: EXACT_MATCH_CONFIDENCE });
    }

    #[test]
    fn no_match_is_a_new_person() {
        let people = vec![person(1, "Alice", &[(Channel::Email, "alice@corp.com")])];
        let r = resolve(&Identity::new(Channel::Slack, "bob"), Some("Bob"), &people);
        assert_eq!(r, Resolution::New);
    }

    #[test]
    fn resolution_is_deterministic_first_match_wins() {
        // two people share the identity (shouldn't happen, but be deterministic) → lowest input idx
        let people = vec![
            person(3, "A", &[(Channel::GitHub, "octocat")]),
            person(7, "B", &[(Channel::GitHub, "octocat")]),
        ];
        let r = resolve(&Identity::new(Channel::GitHub, "octocat"), None, &people);
        assert_eq!(r, Resolution::Match { person_id: 3, confidence: EXACT_MATCH_CONFIDENCE });
    }

    #[test]
    fn empty_seen_name_does_not_match_empty_display_name() {
        let people = vec![person(1, "", &[(Channel::Email, "x@y.com")])];
        // a blank incoming name must not collide with a blank stored name.
        assert_eq!(resolve(&Identity::new(Channel::Slack, "z"), Some("   "), &people), Resolution::New);
    }
}

// ------------------------------------------------------------------ persistence

use rusqlite::{params, Connection};

impl Channel {
    /// The stored tag for a handle. Handles are persisted as `"<channel>:<value>"` so a Slack
    /// `alice` and a GitHub `alice` stay distinguishable — storing bare handles would let them
    /// collide into one person, which is exactly the mis-merge this module exists to prevent.
    pub fn tag(self) -> &'static str {
        match self {
            Channel::Email => "email",
            Channel::Slack => "slack",
            Channel::GitHub => "github",
            Channel::Linear => "linear",
            Channel::Notion => "notion",
        }
    }

    pub fn from_tag(s: &str) -> Option<Self> {
        match s {
            "email" => Some(Channel::Email),
            "slack" => Some(Channel::Slack),
            "github" => Some(Channel::GitHub),
            "linear" => Some(Channel::Linear),
            "notion" => Some(Channel::Notion),
            _ => None,
        }
    }
}

fn parse_json_array(raw: Option<String>) -> Vec<String> {
    raw.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok()).unwrap_or_default()
}

/// Project every `people` row into the identity fingerprint [`resolve`] compares against.
pub fn known_people(conn: &Connection) -> Result<Vec<PersonIdentities>, rusqlite::Error> {
    let mut stmt = conn.prepare("SELECT id, display_name, emails, handles FROM people ORDER BY id")?;
    let rows = stmt.query_map([], |r| {
        let id: i64 = r.get(0)?;
        let display_name: String = r.get(1)?;
        let mut identities: Vec<Identity> = parse_json_array(r.get(2)?)
            .into_iter()
            .map(|e| Identity::new(Channel::Email, e))
            .collect();
        for h in parse_json_array(r.get(3)?) {
            if let Some((tag, value)) = h.split_once(':') {
                if let Some(ch) = Channel::from_tag(tag) {
                    identities.push(Identity::new(ch, value));
                }
            }
        }
        Ok(PersonIdentities { person_id: id, display_name, identities })
    })?;
    rows.collect()
}

/// Attach an identity to an existing person, if they do not already carry it.
fn attach(
    conn: &Connection,
    person_id: i64,
    incoming: &Identity,
    now_ms: i64,
) -> Result<(), rusqlite::Error> {
    let column = if incoming.channel == Channel::Email { "emails" } else { "handles" };
    let stored = if incoming.channel == Channel::Email {
        incoming.value.trim().to_string()
    } else {
        format!("{}:{}", incoming.channel.tag(), incoming.value.trim())
    };
    let raw: Option<String> = conn.query_row(
        &format!("SELECT {column} FROM people WHERE id = ?1"),
        params![person_id],
        |r| r.get(0),
    )?;
    let mut values = parse_json_array(raw);
    if values.iter().any(|v| v.eq_ignore_ascii_case(&stored)) {
        return Ok(()); // already known
    }
    values.push(stored);
    let json = serde_json::to_string(&values).unwrap_or_else(|_| "[]".into());
    conn.execute(
        &format!("UPDATE people SET {column} = ?1, updated_at = ?2, last_evidence_at = ?2 WHERE id = ?3"),
        params![json, now_ms, person_id],
    )?;
    Ok(())
}

/// The effect of observing one identity.
#[derive(Debug, Clone, PartialEq)]
pub enum Observed {
    /// Merged into an existing person (exact channel-identity match).
    Merged { person_id: i64 },
    /// A weak name-only overlap: recorded as a NEW person, with the possible match reported so it
    /// can be offered for confirmation. Never merged automatically — un-fusing two real people is
    /// disruptive, while a missed merge is easy to fix later.
    NewWithPossibleMatch { person_id: i64, possible: i64 },
    /// Nothing resembled it; a new person.
    New { person_id: i64 },
}

/// Observe an identity seen in an event, merging it into the right person or creating one.
///
/// `event_id` becomes the new person's provenance (FR-ST-02 requires every state row to have
/// some), and `seen_name` is the display name it appeared under.
pub fn observe(
    conn: &mut Connection,
    incoming: &Identity,
    seen_name: Option<&str>,
    event_id: i64,
    now_ms: i64,
) -> Result<Observed, crate::MemoryError> {
    let people = known_people(conn)?;
    let resolution = resolve(incoming, seen_name, &people);
    match resolution {
        Resolution::Match { person_id, .. } => {
            attach(conn, person_id, incoming, now_ms)?;
            Ok(Observed::Merged { person_id })
        }
        Resolution::Possible { person_id: possible, .. } => {
            let id = create(conn, incoming, seen_name, event_id, now_ms)?;
            Ok(Observed::NewWithPossibleMatch { person_id: id, possible })
        }
        Resolution::New => {
            let id = create(conn, incoming, seen_name, event_id, now_ms)?;
            Ok(Observed::New { person_id: id })
        }
    }
}

fn create(
    conn: &mut Connection,
    incoming: &Identity,
    seen_name: Option<&str>,
    event_id: i64,
    now_ms: i64,
) -> Result<i64, crate::MemoryError> {
    let name = seen_name.map(str::trim).filter(|s| !s.is_empty()).unwrap_or(&incoming.value);
    let id = crate::state::insert_person(
        conn,
        &crate::state::NewPerson {
            display_name: name,
            // A person known only from one sighting is a weak record, and says so.
            confidence: 0.4,
            now: now_ms,
            ..Default::default()
        },
        &[crate::state::Provenance::new(event_id)],
    )?;
    attach(conn, id, incoming, now_ms)?;
    Ok(id)
}

#[cfg(test)]
mod persistence_tests {
    use super::*;

    fn seed_event(conn: &Connection) -> i64 {
        crate::event_log::insert(
            conn,
            &crate::event_log::NewEvent {
                ts: 1,
                source: "gmail",
                kind: "email",
                app_bundle_id: None,
                window_title: Some("thread"),
                content: "hello",
                content_hash: "h1",
                dwell_ms: 0,
                display_id: None,
                window_bounds: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn the_same_address_seen_twice_is_one_person() {
        let mut conn = crate::open_in_memory().unwrap();
        let e = seed_event(&conn);
        let alice = Identity::new(Channel::Email, "alice@example.com");
        let first = observe(&mut conn, &alice, Some("Alice"), e, 1).unwrap();
        let second =
            observe(&mut conn, &Identity::new(Channel::Email, "ALICE@example.com"), None, e, 2)
                .unwrap();
        let Observed::New { person_id } = first else { panic!("first sighting is new: {first:?}") };
        assert_eq!(second, Observed::Merged { person_id }, "case must not fork the person");

        let n: i64 = conn.query_row("SELECT count(*) FROM people", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    /// The point of cross-channel resolution: one person, several systems.
    #[test]
    fn a_person_accumulates_identities_across_channels() {
        let mut conn = crate::open_in_memory().unwrap();
        let e = seed_event(&conn);
        let alice = Identity::new(Channel::Email, "alice@example.com");
        let Observed::New { person_id } = observe(&mut conn, &alice, Some("Alice"), e, 1).unwrap()
        else {
            panic!("new")
        };
        // Her Slack handle is not yet known, so it arrives as its own record…
        observe(&mut conn, &Identity::new(Channel::Slack, "alice"), Some("Alice"), e, 2).unwrap();
        // …but once the Slack handle is attached to her, seeing it again resolves to her.
        attach(&conn, person_id, &Identity::new(Channel::Slack, "alice"), 3).unwrap();
        let again = observe(&mut conn, &Identity::new(Channel::Slack, "alice"), None, e, 4).unwrap();
        assert_eq!(again, Observed::Merged { person_id });
    }

    /// A Slack `alice` and a GitHub `alice` are not the same identity — storing bare handles would
    /// fuse two people, which is the failure this module is built to avoid.
    #[test]
    fn the_same_handle_on_different_platforms_stays_separate() {
        let mut conn = crate::open_in_memory().unwrap();
        let e = seed_event(&conn);
        let a = observe(&mut conn, &Identity::new(Channel::Slack, "alice"), Some("A"), e, 1).unwrap();
        let b = observe(&mut conn, &Identity::new(Channel::GitHub, "alice"), Some("B"), e, 2).unwrap();
        let (Observed::New { person_id: pa }, Observed::New { person_id: pb }) = (a, b) else {
            panic!("both should be new people")
        };
        assert_ne!(pa, pb, "different platforms must not fuse");
    }

    /// A name collision is reported, never acted on.
    #[test]
    fn a_name_only_overlap_is_offered_not_merged() {
        let mut conn = crate::open_in_memory().unwrap();
        let e = seed_event(&conn);
        let Observed::New { person_id: first } =
            observe(&mut conn, &Identity::new(Channel::Email, "alice@a.com"), Some("Alice"), e, 1)
                .unwrap()
        else {
            panic!("new")
        };
        // A different address, same display name: plausibly the same person, plausibly not.
        let second =
            observe(&mut conn, &Identity::new(Channel::Email, "alice@b.com"), Some("Alice"), e, 2)
                .unwrap();
        match second {
            Observed::NewWithPossibleMatch { person_id, possible } => {
                assert_ne!(person_id, first, "kept separate");
                assert_eq!(possible, first, "and the overlap is reported for confirmation");
            }
            other => panic!("a name collision must not auto-merge: {other:?}"),
        }
    }

    #[test]
    fn attaching_a_known_identity_twice_does_not_duplicate_it() {
        let mut conn = crate::open_in_memory().unwrap();
        let e = seed_event(&conn);
        let alice = Identity::new(Channel::Email, "alice@example.com");
        let Observed::New { person_id } = observe(&mut conn, &alice, Some("Alice"), e, 1).unwrap()
        else {
            panic!("new")
        };
        attach(&conn, person_id, &alice, 2).unwrap();
        let raw: Option<String> = conn
            .query_row("SELECT emails FROM people WHERE id = ?1", [person_id], |r| r.get(0))
            .unwrap();
        assert_eq!(parse_json_array(raw).len(), 1, "no duplicate entry");
    }
}
