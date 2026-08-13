-- The offer context around a feedback decision (FR-PAT-01 / FR-CF-03 supply). Additive: four
-- nullable columns on `feedback_events` plus one index.
--
-- V16 records WHAT the user decided (kind) and, for edits, the before/after text the lessons
-- distil from. What it cannot answer is anything about the offer itself: where the proposal was
-- shown, what slot it sat in, what the user was doing at the time, how long they took. Those are
-- the inputs FR-CF-03's priority score names ("直近の同種アクション採択率" needs a per-kind
-- adoption rate, and a rate is only meaningful once you can tell a top-slot offer from a
-- fifth-slot one).
--
-- Metadata only, deliberately. These columns carry an app bundle id and three numbers — never the
-- action's content, never captured text. Same rule as the rest of the table: no egress path
-- touches it and nothing logs it.
--
-- All four are NULLable with no default, so every existing row stays valid and readers must treat
-- "not recorded" as its own answer rather than inventing a zero. `surface` carries a CHECK (legal
-- on ADD COLUMN in SQLite as long as it admits NULL) so the vocabulary stays closed and the
-- aggregation stays meaningful — a typo silently becoming a valid surface would corrupt exactly
-- the statistics this exists to produce.
--
-- ロールバック手順:
--   1. SQLite は DROP COLUMN を 3.35 以降でサポートするが、CHECK 付きの列を含むため
--      テーブル再作成のほうが確実（docs/migrations/V19-rollback.md に手順）
--   2. refinery_schema_history から version = 19 の行を削除する

ALTER TABLE feedback_events ADD COLUMN surface TEXT
    CHECK (surface IS NULL OR surface IN ('notch', 'option_key', 'chat', 'recap', 'api'));

-- The candidate's position when offered (0 = top slot). NULL when the surface does not rank.
ALTER TABLE feedback_events ADD COLUMN rank INTEGER;

-- Frontmost app at decision time (bundle id only — context, not content).
ALTER TABLE feedback_events ADD COLUMN context_app TEXT;

-- Offer → decision, for "was this proposal even considered". NULL when unknown.
ALTER TABLE feedback_events ADD COLUMN latency_ms INTEGER;

-- acceptance_by_kind groups by action_kind over a time window; without this it is a full scan of
-- a table that grows with every decision the user ever makes.
CREATE INDEX idx_feedback_events_kind_ts ON feedback_events (action_kind, ts_ms);
