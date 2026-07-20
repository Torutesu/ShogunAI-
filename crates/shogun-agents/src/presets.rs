//! The seven preset agents (WP3.3, §6.6.3). v1 defines them in code (FR-AG-10..16).
//!
//! Each preset is a fixed table of *operations*, and every operation declares both its effect
//! [`OpKind`] and its permission [`Level`]. Two acceptance rules from §6.6 are enforced here as
//! tests over this table:
//! - **Coverage** — every operation has a level (guaranteed structurally; no operation without one).
//! - **Consistency with the permission model** — the declared level always matches the level the
//!   effect kind mandates ([`OpKind::mandated_level`]). In particular every [`OpKind::ExternalSend`]
//!   is [`Level::L3`] (invariant 4): a preset cannot declare an auto-run send.
//!
//! Presentation (Fusion) happens on Standard too (FR-CF-05); *execution* is Pro/BYOK. This module
//! is only the static definition — gating at run time is [`crate::engine`].

use crate::permission::Level;

/// The kind of effect an operation has. This is what fixes its permission level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    /// On-device computation/aggregation, no external I/O (L1).
    LocalCompute,
    /// Generate a draft locally (never sends) — the draft-stop default (L2).
    DraftGenerate,
    /// Write to local state tables after approval (L2).
    StateWrite,
    /// Read from an external service (e.g. calendar free/busy). Not a send, but leaves the device
    /// to read, so it is confirmed (L2).
    ExternalRead,
    /// Send/post/create off the device — always explicit confirmation (L3, invariant 4).
    ExternalSend,
}

impl OpKind {
    /// The permission level this effect kind requires. The single source of truth the preset
    /// tables are checked against.
    pub fn mandated_level(self) -> Level {
        match self {
            OpKind::LocalCompute => Level::L1,
            OpKind::DraftGenerate | OpKind::StateWrite | OpKind::ExternalRead => Level::L2,
            OpKind::ExternalSend => Level::L3,
        }
    }

    /// Whether this effect leaves the device as a write (a send). Sends are always L3.
    pub fn is_external_send(self) -> bool {
        matches!(self, OpKind::ExternalSend)
    }
}

/// One operation a preset can perform, with its effect kind and required level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Operation {
    pub name: &'static str,
    pub kind: OpKind,
    pub level: Level,
}

impl Operation {
    /// Declare an operation whose level is taken from its effect kind, so the table cannot drift
    /// from the permission model.
    const fn new(name: &'static str, kind: OpKind) -> Self {
        // `mandated_level` is not const-callable through the match above in a readable way, so the
        // level is set explicitly and a test asserts it equals `kind.mandated_level()`.
        let level = match kind {
            OpKind::LocalCompute => Level::L1,
            OpKind::DraftGenerate | OpKind::StateWrite | OpKind::ExternalRead => Level::L2,
            OpKind::ExternalSend => Level::L3,
        };
        Self { name, kind, level }
    }
}

/// The seven preset ids (§6.6.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetId {
    ReplyDrafter,
    MeetingPrep,
    TaskExtractor,
    FollowUpSentinel,
    CalendarScheduler,
    IssueTriage,
    NoteCapture,
}

/// A preset agent definition.
#[derive(Debug, Clone, Copy)]
pub struct Preset {
    pub id: PresetId,
    /// FR number in the requirements (§6.6.3 table).
    pub fr: &'static str,
    /// Human name (UI string; English per CLAUDE.md).
    pub name: &'static str,
    /// The operations this preset can perform, each level-tagged.
    pub operations: &'static [Operation],
}

use OpKind::*;

/// FR-AG-10 Reply Drafter: draft a reply to the focused mail/Slack thread; send is Gmail-only via
/// Composio (opt-in).
const REPLY_DRAFTER: &[Operation] = &[
    Operation::new("draft_reply", DraftGenerate),
    Operation::new("send_reply", ExternalSend),
];

/// FR-AG-11 Meeting Prep: aggregate participant/agenda state into a local briefing; optional LLM
/// formatting is an L2 launch.
const MEETING_PREP: &[Operation] = &[
    Operation::new("aggregate_brief", LocalCompute),
    Operation::new("llm_format", DraftGenerate),
];

/// FR-AG-12 Task Extractor: extract tasks/commitments from screen/thread text and propose adding
/// them to open_loops/commitments (approval writes state).
const TASK_EXTRACTOR: &[Operation] = &[Operation::new("extract_to_state", StateWrite)];

/// FR-AG-13 Follow-up Sentinel: surface overdue commitments / stale open loops (Dream Cycle
/// output), draft a follow-up, optionally send/post.
const FOLLOW_UP_SENTINEL: &[Operation] = &[
    Operation::new("detect_and_show", LocalCompute),
    Operation::new("draft_follow_up", DraftGenerate),
    Operation::new("send_follow_up", ExternalSend),
];

/// FR-AG-14 Calendar Scheduler: read free/busy (L2) and propose a Google Calendar event; creating
/// the event is irreversible → L3 (invariant 4).
const CALENDAR_SCHEDULER: &[Operation] = &[
    Operation::new("read_free_busy", ExternalRead),
    Operation::new("create_event", ExternalSend),
];

/// FR-AG-15 Issue Triage: draft a GitHub/Linear issue/comment (L2); creating/posting is L3.
const ISSUE_TRIAGE: &[Operation] = &[
    Operation::new("draft_issue", DraftGenerate),
    Operation::new("post_issue_or_comment", ExternalSend),
];

/// FR-AG-16 Note Capture: draft a Notion page/row (L2); writing to Notion is L3.
const NOTE_CAPTURE: &[Operation] = &[
    Operation::new("draft_note", DraftGenerate),
    Operation::new("write_to_notion", ExternalSend),
];

/// The seven presets, in FR order.
pub const PRESETS: &[Preset] = &[
    Preset { id: PresetId::ReplyDrafter, fr: "FR-AG-10", name: "Reply Drafter", operations: REPLY_DRAFTER },
    Preset { id: PresetId::MeetingPrep, fr: "FR-AG-11", name: "Meeting Prep", operations: MEETING_PREP },
    Preset { id: PresetId::TaskExtractor, fr: "FR-AG-12", name: "Task Extractor", operations: TASK_EXTRACTOR },
    Preset {
        id: PresetId::FollowUpSentinel,
        fr: "FR-AG-13",
        name: "Follow-up Sentinel",
        operations: FOLLOW_UP_SENTINEL,
    },
    Preset {
        id: PresetId::CalendarScheduler,
        fr: "FR-AG-14",
        name: "Calendar Scheduler",
        operations: CALENDAR_SCHEDULER,
    },
    Preset { id: PresetId::IssueTriage, fr: "FR-AG-15", name: "Issue Triage", operations: ISSUE_TRIAGE },
    Preset { id: PresetId::NoteCapture, fr: "FR-AG-16", name: "Note Capture", operations: NOTE_CAPTURE },
];

/// Look up a preset by id.
pub fn preset(id: PresetId) -> &'static Preset {
    // PRESETS covers every variant (asserted in tests), so this always finds one; the fallback
    // keeps the function total without `unwrap`.
    PRESETS.iter().find(|p| p.id == id).unwrap_or(&PRESETS[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_exactly_seven_presets() {
        assert_eq!(PRESETS.len(), 7);
    }

    #[test]
    fn every_operation_level_matches_its_effect_kind() {
        // §6.6 acceptance: the permission table is exhaustive and consistent — every operation has
        // a level, and it is the one its effect kind mandates.
        for p in PRESETS {
            assert!(!p.operations.is_empty(), "{} has no operations", p.name);
            for op in p.operations {
                assert_eq!(
                    op.level,
                    op.kind.mandated_level(),
                    "{}::{} level {:?} disagrees with kind {:?}",
                    p.name,
                    op.name,
                    op.level,
                    op.kind
                );
            }
        }
    }

    #[test]
    fn every_external_send_is_l3_and_no_l1_send_exists() {
        // Invariant 4 over the preset tables: a send is always L3, and no L1 operation is a send.
        for p in PRESETS {
            for op in p.operations {
                if op.kind.is_external_send() {
                    assert_eq!(op.level, Level::L3, "{}::{} send must be L3", p.name, op.name);
                }
                if op.level == Level::L1 {
                    assert!(!op.kind.is_external_send(), "{}::{} L1 must not be a send", p.name, op.name);
                }
            }
        }
    }

    #[test]
    fn presets_that_can_send_all_have_an_l3_operation() {
        // The four presets whose §6.6.3 rows include a send/post/create must expose exactly that as
        // L3 — never silently downgraded.
        for id in [
            PresetId::ReplyDrafter,
            PresetId::FollowUpSentinel,
            PresetId::CalendarScheduler,
            PresetId::IssueTriage,
            PresetId::NoteCapture,
        ] {
            let p = preset(id);
            assert!(
                p.operations.iter().any(|op| op.level == Level::L3),
                "{} should have an L3 send operation",
                p.name
            );
        }
    }

    #[test]
    fn local_only_presets_have_no_send() {
        // Meeting Prep and Task Extractor never leave the device with a write.
        for id in [PresetId::MeetingPrep, PresetId::TaskExtractor] {
            let p = preset(id);
            assert!(
                p.operations.iter().all(|op| !op.kind.is_external_send()),
                "{} must not have a send operation",
                p.name
            );
        }
    }

    #[test]
    fn preset_lookup_is_total_over_all_ids() {
        for id in [
            PresetId::ReplyDrafter,
            PresetId::MeetingPrep,
            PresetId::TaskExtractor,
            PresetId::FollowUpSentinel,
            PresetId::CalendarScheduler,
            PresetId::IssueTriage,
            PresetId::NoteCapture,
        ] {
            assert_eq!(preset(id).id, id, "lookup returned the wrong preset");
        }
    }
}
