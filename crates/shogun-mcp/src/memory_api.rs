//! The Memory API surface (§6.11, FR-API-01..06) — the AI-facing access to SHOGUN's memory, kept
//! **completely symmetric with the human UI** (invariant 6). The three faces (MCP / CLI / REST) are
//! thin wrappers over the same internal API, so the policy lives here once:
//! - the v1 tool set and each tool's permission level (FR-API-02), reusing the same L1/L2/L3 as the
//!   UI (FR-API-04 — an API-driven L3 goes through the very same approval flow);
//! - token auth (FR-API-03): no token → everything denied, reads included;
//! - the read confidence rule (FR-API-06): <0.5 excluded by default, `possibly` flag on medium,
//!   low included only when explicitly requested — the same [`shogun_fusion::confidence`] bands the
//!   UI uses.

use shogun_agents::permission::Level;
use shogun_fusion::confidence::{band, Band};

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
    /// The device's onboarding / first-run setup state (issue #6). A read: an agent needs to know
    /// how far this device is configured, symmetrically with the human UI (invariant 6).
    DeviceOnboardingGet,
    VisualRecallStatus,
    VisualRecallSetEnabled,
    VisualRecallSearchFrames,
    VisualRecallGetFrame,
    VisualRecallRescanFrame,
    VisualRecallDeleteFrame,
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
    Tool::DeviceOnboardingGet,
    Tool::VisualRecallStatus,
    Tool::VisualRecallSetEnabled,
    Tool::VisualRecallSearchFrames,
    Tool::VisualRecallGetFrame,
    Tool::VisualRecallRescanFrame,
    Tool::VisualRecallDeleteFrame,
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
            Tool::DeviceOnboardingGet => "device.onboarding.get",
            Tool::VisualRecallStatus => "visual_recall.status",
            Tool::VisualRecallSetEnabled => "visual_recall.set_enabled",
            Tool::VisualRecallSearchFrames => "visual_recall.search_frames",
            Tool::VisualRecallGetFrame => "visual_recall.get_frame",
            Tool::VisualRecallRescanFrame => "visual_recall.rescan_frame",
            Tool::VisualRecallDeleteFrame => "visual_recall.delete_frame",
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
        | Tool::DeviceOnboardingGet
        | Tool::VisualRecallStatus
        | Tool::VisualRecallSearchFrames
        | Tool::VisualRecallGetFrame
        | Tool::VisualRecallRescanFrame => ApiLevel::Read,
        // append a user note to the event log — local, reversible.
        Tool::MemoryAppendNote => ApiLevel::Write(Level::L1),
        // visual recall master switch — same as Settings toggle (L1, local).
        Tool::VisualRecallSetEnabled => ApiLevel::Write(Level::L1),
        // Deleting a local frame is an L1 write.
        Tool::VisualRecallDeleteFrame => ApiLevel::Write(Level::L1),
        // propose a state change — one-tap confirm in the Notch.
        Tool::StateProposeUpdate => ApiLevel::Write(Level::L2),
        // launch a preset agent — level follows the action it runs.
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
    valid: Vec<String>,
}

impl TokenRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Issue a token id (Full UI issuance).
    pub fn issue(&mut self, token_id: impl Into<String>) {
        self.valid.push(token_id.into());
    }

    /// Revoke a token id (Full UI revocation).
    pub fn revoke(&mut self, token_id: &str) {
        self.valid.retain(|t| t != token_id);
    }

    /// Authenticate a call. `None` (no token presented) is denied — reads included (FR-API-03).
    pub fn authenticate(&self, token_id: Option<&str>) -> AuthResult {
        match token_id {
            None => AuthResult::DeniedNoToken,
            Some(t) if self.valid.iter().any(|v| v == t) => AuthResult::Granted,
            Some(_) => AuthResult::DeniedInvalidToken,
        }
    }
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
        assert_eq!(tool_level(Tool::MemoryAppendNote), ApiLevel::Write(Level::L1));
        assert_eq!(tool_level(Tool::StateProposeUpdate), ApiLevel::Write(Level::L2));
        assert_eq!(tool_level(Tool::ActionsExecute), ApiLevel::PerAction);
    }

    #[test]
    fn onboarding_state_is_a_symmetric_read_tool() {
        // Invariant 6 (issue #6): the device's onboarding/first-run state is readable from the
        // agent side, on the same surface as every other read, at Read level.
        assert_eq!(Tool::from_wire("device.onboarding.get"), Some(Tool::DeviceOnboardingGet));
        assert_eq!(Tool::DeviceOnboardingGet.wire_name(), "device.onboarding.get");
        assert_eq!(tool_level(Tool::DeviceOnboardingGet), ApiLevel::Read);
        assert!(ALL_TOOLS.contains(&Tool::DeviceOnboardingGet));
    }

    #[test]
    fn every_tool_has_a_defined_level() {
        // FR-API-02: the tool set is fully specified — no tool without a level.
        for &t in ALL_TOOLS {
            let _ = tool_level(t); // exhaustive match means this cannot be undefined
        }
        assert_eq!(ALL_TOOLS.len(), 20);
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
                        | Tool::DeviceOnboardingGet
                        | Tool::VisualRecallStatus
                        | Tool::VisualRecallSearchFrames
                        | Tool::VisualRecallGetFrame
                        | Tool::VisualRecallRescanFrame
                )),
                ApiLevel::Write(_) => {
                    assert!(matches!(
                        t,
                        Tool::MemoryAppendNote
                            | Tool::StateProposeUpdate
                            | Tool::VisualRecallSetEnabled
                            | Tool::VisualRecallDeleteFrame
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
        assert_eq!(reg.authenticate(Some("unknown")), AuthResult::DeniedInvalidToken);
        reg.revoke("client-abc");
        assert_eq!(reg.authenticate(Some("client-abc")), AuthResult::DeniedInvalidToken);
    }

    #[test]
    fn read_confidence_rule_matches_ui() {
        // High plain, Medium possibly, Low excluded by default.
        assert_eq!(read_inclusion(0.9, false), ReadInclusion::Included { possibly: false });
        assert_eq!(read_inclusion(0.6, false), ReadInclusion::Included { possibly: true });
        assert_eq!(read_inclusion(0.3, false), ReadInclusion::Excluded);
        // low included only on explicit opt-in, flagged possibly.
        assert_eq!(read_inclusion(0.3, true), ReadInclusion::Included { possibly: true });
    }
}
