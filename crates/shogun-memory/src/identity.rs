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
