//! The Memory API surface (§6.11, FR-API-01..06) — the AI-facing access to SHOGUN's memory, kept
//! **completely symmetric with the human UI** (invariant 6). The three faces (MCP / CLI / REST) are
//! thin wrappers over the same internal API, so the policy lives here once:
//! - the v1 tool set and each tool's permission level (FR-API-02), reusing the same L1/L2/L3 as the
//!   UI (FR-API-04 — an API-driven L3 goes through the very same approval flow);
//! - token auth (FR-API-03): no token → everything denied, reads included;
//! - the read confidence rule (FR-API-06): <0.5 excluded by default, `possibly` flag on medium,
//!   low included only when explicitly requested — the same [`shogun_fusion::confidence`] bands the
//!   UI uses.

use sha2::{Digest, Sha256};
use shogun_agents::permission::Level;
use shogun_fusion::confidence::{band, Band};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use subtle::ConstantTimeEq;

/// The v1 Memory API tools (FR-API-02). Symmetric with the UI's corresponding features.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    MemorySearch,
    MemoryGetContext,
    StatePeopleList,
    StatePeopleGet,
    StateProjectsList,
    StateProjectsGet,
    StateCommitmentsList,
    StateCommitmentsGet,
    StateOpenLoopsList,
    StateOpenLoopsGet,
    MemoryAppendNote,
    StateProposeUpdate,
    ActionsExecute,
    VisualRecallStatus,
    VisualRecallSetEnabled,
    VisualRecallSetRetention,
    VisualRecallSearchFrames,
    VisualRecallGetFrame,
    VisualRecallRescanFrame,
    VisualRecallDeleteFrame,
    /// Profile + short work summary (people/projects/commitments/open_loops counts).
    ProfileWhoami,
    /// Update user-owned profile prefs (L1).
    ProfileSet,
}

/// Every tool, for exhaustive iteration (settings / tests).
pub const ALL_TOOLS: &[Tool] = &[
    Tool::MemorySearch,
    Tool::MemoryGetContext,
    Tool::StatePeopleList,
    Tool::StatePeopleGet,
    Tool::StateProjectsList,
    Tool::StateProjectsGet,
    Tool::StateCommitmentsList,
    Tool::StateCommitmentsGet,
    Tool::StateOpenLoopsList,
    Tool::StateOpenLoopsGet,
    Tool::MemoryAppendNote,
    Tool::StateProposeUpdate,
    Tool::ActionsExecute,
    Tool::VisualRecallStatus,
    Tool::VisualRecallSetEnabled,
    Tool::VisualRecallSetRetention,
    Tool::VisualRecallSearchFrames,
    Tool::VisualRecallGetFrame,
    Tool::VisualRecallRescanFrame,
    Tool::VisualRecallDeleteFrame,
    Tool::ProfileWhoami,
    Tool::ProfileSet,
];

impl Tool {
    /// The stable wire name (the same string the CLI, REST, and MCP faces use).
    pub fn wire_name(self) -> &'static str {
        match self {
            Tool::MemorySearch => "memory.search",
            Tool::MemoryGetContext => "memory.get_context",
            Tool::StatePeopleList => "state.people.list",
            Tool::StatePeopleGet => "state.people.get",
            Tool::StateProjectsList => "state.projects.list",
            Tool::StateProjectsGet => "state.projects.get",
            Tool::StateCommitmentsList => "state.commitments.list",
            Tool::StateCommitmentsGet => "state.commitments.get",
            Tool::StateOpenLoopsList => "state.open_loops.list",
            Tool::StateOpenLoopsGet => "state.open_loops.get",
            Tool::MemoryAppendNote => "memory.append_note",
            Tool::StateProposeUpdate => "state.propose_update",
            Tool::ActionsExecute => "actions.execute",
            Tool::VisualRecallStatus => "visual_recall.status",
            Tool::VisualRecallSetEnabled => "visual_recall.set_enabled",
            Tool::VisualRecallSetRetention => "visual_recall.set_retention",
            Tool::VisualRecallSearchFrames => "visual_recall.search_frames",
            Tool::VisualRecallGetFrame => "visual_recall.get_frame",
            Tool::VisualRecallRescanFrame => "visual_recall.rescan_frame",
            Tool::VisualRecallDeleteFrame => "visual_recall.delete_frame",
            Tool::ProfileWhoami => "profile.whoami",
            Tool::ProfileSet => "profile.set",
        }
    }

    /// Parse a wire name back to a tool.
    pub fn from_wire(name: &str) -> Option<Tool> {
        ALL_TOOLS.iter().copied().find(|t| t.wire_name() == name)
    }
}

/// A tool's permission requirement (FR-API-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiLevel {
    /// A read (still requires a valid token, FR-API-03; confidence rule applies, FR-API-06).
    Read,
    /// A write gated at this level (append_note = L1, propose_update = L2).
    Write(Level),
    /// `actions.execute` — the level is that of the launched action (L3 sends use the approval flow).
    PerAction,
}

/// The permission requirement for a tool (FR-API-02 table).
pub fn tool_level(tool: Tool) -> ApiLevel {
    match tool {
        Tool::MemorySearch
        | Tool::MemoryGetContext
        | Tool::StatePeopleList
        | Tool::StatePeopleGet
        | Tool::StateProjectsList
        | Tool::StateProjectsGet
        | Tool::StateCommitmentsList
        | Tool::StateCommitmentsGet
        | Tool::StateOpenLoopsList
        | Tool::StateOpenLoopsGet
        | Tool::VisualRecallStatus
        | Tool::VisualRecallSearchFrames
        | Tool::VisualRecallGetFrame
        | Tool::VisualRecallRescanFrame
        | Tool::ProfileWhoami => ApiLevel::Read,
        // append a user note to the event log — local, reversible.
        Tool::MemoryAppendNote => ApiLevel::Write(Level::L1),
        // visual recall master switch — same as Settings toggle (L1, local).
        Tool::VisualRecallSetEnabled | Tool::VisualRecallSetRetention => ApiLevel::Write(Level::L1),
        // Deleting a local frame is an L1 write.
        Tool::VisualRecallDeleteFrame => ApiLevel::Write(Level::L1),
        // user-owned profile prefs text — explicit set, like Unabyss store (L1).
        Tool::ProfileSet => ApiLevel::Write(Level::L1),
        // propose a state change — one-tap confirm in the Notch.
        Tool::StateProposeUpdate => ApiLevel::Write(Level::L2),
        // launch a preset agent — level follows the action it runs.
        // v2: listed for symmetry; shared Notch approval queue not wired for standalone MCP yet.
        Tool::ActionsExecute => ApiLevel::PerAction,
    }
}

// ---- auth (FR-API-03) ----------------------------------------------------------------------

/// The result of authenticating an API call. No token → denied, reads included (FR-API-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthResult {
    Granted,
    DeniedNoToken,
    DeniedInvalidToken,
}

/// Per-client API tokens (FR-API-03). Real tokens live in the Keychain; this registry holds only
/// opaque identifiers for validation. REST/HTTP binds to localhost (NFR-SEC-03) — a runtime concern
/// outside this pure model.
#[derive(Default)]
pub struct TokenRegistry {
    valid: Vec<[u8; 32]>,
    #[cfg(test)]
    last_scan_count: AtomicUsize,
}

impl TokenRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Issue a token id (Full UI issuance).
    pub fn issue(&mut self, token: impl Into<String>) {
        let token = token.into();
        self.valid.push(verifier_bytes(&token));
    }

    /// Load a persisted verifier. This is trusted storage input, never bearer input.
    pub fn issue_verifier(&mut self, verifier: &str) -> Result<(), String> {
        self.valid
            .push(crate::memory_api_settings::persisted_verifier_bytes(
                verifier,
            )?);
        Ok(())
    }

    /// Whether any tokens are registered.
    pub fn is_empty(&self) -> bool {
        self.valid.is_empty()
    }

    /// Revoke a token id (Full UI revocation).
    pub fn revoke(&mut self, token: &str) {
        let digest = verifier_bytes(token);
        self.valid
            .retain(|stored| stored.ct_eq(&digest).unwrap_u8() == 0);
    }

    /// Authenticate a call. `None` (no token presented) is denied — reads included (FR-API-03).
    pub fn authenticate(&self, token: Option<&str>) -> AuthResult {
        match token {
            None => AuthResult::DeniedNoToken,
            Some(t) => {
                let digest = verifier_bytes(t);
                let presented_verifier =
                    t.starts_with(crate::memory_api_settings::TOKEN_VERIFIER_PREFIX);
                let mut matched = 0u8;
                #[cfg(test)]
                self.last_scan_count.store(0, Ordering::Relaxed);
                for stored in &self.valid {
                    matched |= stored.ct_eq(&digest).unwrap_u8();
                    #[cfg(test)]
                    self.last_scan_count.fetch_add(1, Ordering::Relaxed);
                }
                if matched == 1 && !presented_verifier {
                    AuthResult::Granted
                } else {
                    AuthResult::DeniedInvalidToken
                }
            }
        }
    }

    /// Authenticate process startup. Empty registry preserves dev/process trust; once a persisted
    /// token exists, an env bearer must match it. This is distinct from per-call auth, where no
    /// token is always denied.
    pub fn authenticate_process(&self, token: Option<&str>) -> bool {
        self.is_empty() || matches!(self.authenticate(token), AuthResult::Granted)
    }

    #[cfg(test)]
    fn last_scan_count(&self) -> usize {
        self.last_scan_count.load(Ordering::Relaxed)
    }
}

fn verifier_bytes(token_or_verifier: &str) -> [u8; 32] {
    Sha256::digest(token_or_verifier.as_bytes()).into()
}

// ---- read confidence rule (FR-API-06) ------------------------------------------------------

/// Whether/how a read result item is included in an API response (FR-API-06). Mirrors the UI
/// confidence rule (FR-ST-20) so API and UI reads agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadInclusion {
    /// Excluded from the response (Low confidence, and low not explicitly requested).
    Excluded,
    /// Included, with the `possibly` flag the response must carry.
    Included { possibly: bool },
}

/// Decide inclusion for a read item of the given confidence. Low (<0.5) is excluded unless
/// `include_low` is set (explicit opt-in); Medium is included and flagged `possibly`; High is
/// included plainly (FR-API-06).
pub fn read_inclusion(confidence: f64, include_low: bool) -> ReadInclusion {
    match band(confidence) {
        Band::High => ReadInclusion::Included { possibly: false },
        Band::Medium => ReadInclusion::Included { possibly: true },
        Band::Low => {
            if include_low {
                ReadInclusion::Included { possibly: true }
            } else {
                ReadInclusion::Excluded
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_tools_are_reads_writes_carry_expected_levels() {
        assert_eq!(tool_level(Tool::MemorySearch), ApiLevel::Read);
        assert_eq!(tool_level(Tool::StatePeopleGet), ApiLevel::Read);
        assert_eq!(
            tool_level(Tool::MemoryAppendNote),
            ApiLevel::Write(Level::L1)
        );
        assert_eq!(
            tool_level(Tool::StateProposeUpdate),
            ApiLevel::Write(Level::L2)
        );
        assert_eq!(tool_level(Tool::ActionsExecute), ApiLevel::PerAction);
    }

    #[test]
    fn every_tool_has_a_defined_level() {
        // FR-API-02: the tool set is fully specified — no tool without a level.
        for &t in ALL_TOOLS {
            let _ = tool_level(t); // exhaustive match means this cannot be undefined
        }
        assert_eq!(ALL_TOOLS.len(), 21);
    }

    #[test]
    fn no_read_tool_is_a_write_and_vice_versa() {
        for &t in ALL_TOOLS {
            match tool_level(t) {
                ApiLevel::Read => assert!(matches!(
                    t,
                    Tool::MemorySearch
                        | Tool::MemoryGetContext
                        | Tool::StatePeopleList
                        | Tool::StatePeopleGet
                        | Tool::StateProjectsList
                        | Tool::StateProjectsGet
                        | Tool::StateCommitmentsList
                        | Tool::StateCommitmentsGet
                        | Tool::StateOpenLoopsList
                        | Tool::StateOpenLoopsGet
                        | Tool::VisualRecallStatus
                        | Tool::VisualRecallSearchFrames
                        | Tool::VisualRecallGetFrame
                        | Tool::VisualRecallRescanFrame
                        | Tool::ProfileWhoami
                )),
                ApiLevel::Write(_) => {
                    assert!(matches!(
                        t,
                        Tool::MemoryAppendNote
                            | Tool::StateProposeUpdate
                            | Tool::VisualRecallSetEnabled
                            | Tool::VisualRecallSetRetention
                            | Tool::VisualRecallDeleteFrame
                            | Tool::ProfileSet
                    ))
                }
                ApiLevel::PerAction => assert_eq!(t, Tool::ActionsExecute),
            }
        }
    }

    #[test]
    fn no_token_denies_everything_including_reads() {
        let reg = TokenRegistry::new();
        assert_eq!(reg.authenticate(None), AuthResult::DeniedNoToken);
    }

    #[test]
    fn issued_token_grants_and_revoked_token_denies() {
        let mut reg = TokenRegistry::new();
        reg.issue("client-abc");
        assert_eq!(reg.authenticate(Some("client-abc")), AuthResult::Granted);
        assert_eq!(
            reg.authenticate(Some("unknown")),
            AuthResult::DeniedInvalidToken
        );
        reg.revoke("client-abc");
        assert_eq!(
            reg.authenticate(Some("client-abc")),
            AuthResult::DeniedInvalidToken
        );
    }

    #[test]
    fn persisted_verifier_is_not_a_bearer_token_and_scan_never_stops_early() {
        let mut reg = TokenRegistry::new();
        let verifier = crate::memory_api_settings::token_verifier("first");
        reg.issue_verifier(&verifier).unwrap();
        reg.issue("second");
        assert_eq!(
            reg.authenticate(Some(&verifier)),
            AuthResult::DeniedInvalidToken
        );
        assert_eq!(reg.authenticate(Some("first")), AuthResult::Granted);
        assert_eq!(reg.last_scan_count(), 2);
    }

    #[test]
    fn process_auth_requires_persisted_bearer_but_keeps_empty_registry_trust() {
        let mut reg = TokenRegistry::new();
        assert!(reg.authenticate_process(None));

        let verifier = crate::memory_api_settings::token_verifier("correct-token");
        reg.issue_verifier(&verifier).unwrap();
        assert!(!reg.authenticate_process(Some("wrong-token")));
        assert!(reg.authenticate_process(Some("correct-token")));
        assert!(!reg.authenticate_process(Some(&verifier)));
        assert!(!reg.authenticate_process(None));
    }

    #[test]
    fn migrated_legacy_token_authenticates_then_revokes() {
        let old = br#"{"tokens":[{"id":"id","name":"n","created_at":1,"secret":"old-secret"}]}"#;
        let (blob, migrated) =
            crate::memory_api_settings::parse_token_blob_with_migration(old).unwrap();
        assert!(migrated);
        let mut reg = TokenRegistry::new();
        reg.issue_verifier(&blob.tokens[0].verifier).unwrap();
        assert_eq!(reg.authenticate(Some("old-secret")), AuthResult::Granted);
        reg.revoke("old-secret");
        assert_eq!(
            reg.authenticate(Some("old-secret")),
            AuthResult::DeniedInvalidToken
        );
    }

    #[test]
    fn read_confidence_rule_matches_ui() {
        // High plain, Medium possibly, Low excluded by default.
        assert_eq!(
            read_inclusion(0.9, false),
            ReadInclusion::Included { possibly: false }
        );
        assert_eq!(
            read_inclusion(0.6, false),
            ReadInclusion::Included { possibly: true }
        );
        assert_eq!(read_inclusion(0.3, false), ReadInclusion::Excluded);
        // low included only on explicit opt-in, flagged possibly.
        assert_eq!(
            read_inclusion(0.3, true),
            ReadInclusion::Included { possibly: true }
        );
    }
}
