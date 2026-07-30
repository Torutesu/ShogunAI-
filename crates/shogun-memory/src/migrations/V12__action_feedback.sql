-- Learning-input log for the Patterns layer (FR-PAT-01). Additive: one new table.
--
-- Every time the user decides on a proposed action — accepts it, edits it first, dismisses it,
-- confirms a Recap candidate with [Track] or discards it — that decision is the one signal that
-- can later personalize Fusion ranking ("直近の同種アクション採択率" is already an input of the
-- FR-CF-03 priority score) and, in v1.5, tone/pattern learning (FR-PAT-02).
--
-- Deliberately metadata-only: the row records WHAT was decided about (action kind, surface,
-- rank, latency), never the content of the action or the screen. This log never leaves the
-- device (FR-PAT-01), and keeping content out of it makes that promise cheap to audit.

CREATE TABLE action_feedback (
    id          INTEGER PRIMARY KEY,
    ts          INTEGER NOT NULL,
    -- The action's stable kind (the permission-table key, e.g. "draft_reply", "save_note").
    action_kind TEXT    NOT NULL,
    -- Where the decision happened: notch | option_key | chat | recap | api.
    surface     TEXT    NOT NULL,
    -- The decision itself. CHECK keeps the vocabulary closed so aggregation stays meaningful.
    outcome     TEXT    NOT NULL CHECK (outcome IN ('accepted', 'edited', 'dismissed', 'tracked', 'discarded')),
    -- Frontmost app at decision time (bundle id only — context, not content).
    context_app TEXT,
    -- The candidate's position when offered (0 = top slot). NULL when not ranked (e.g. Recap).
    rank        INTEGER,
    -- Offer → decision, for "was this proposal even considered" analysis. NULL when unknown.
    latency_ms  INTEGER
) STRICT;

CREATE INDEX idx_action_feedback_kind_ts ON action_feedback (action_kind, ts);
