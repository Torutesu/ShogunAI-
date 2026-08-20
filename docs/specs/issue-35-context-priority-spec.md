# Issue #35 — Context source priority

Status: implemented for the v1 path below. Does not add a Settings UI (issue non-goal).

## Policy

Trust is a property of **what the row is**, not of “it arrived via MCP.”

| Kind of information | Authoritative source | Fusion `SourceKind` | Write path |
|---|---|---|---|
| Calendar event (id, time, title) | Google Calendar API | `Structured` | Event log only. **No** local-rule extract. |
| Issue / task **state** (when typed ingest exists) | GitHub / Linear API | `Structured` (reserved) | Not wired: Wave 3 ingest is still prose bodies. |
| Mail / Slack / Drive / Notion **body** | The message | `Evidence` | `extract_untrusted` at ≤ 0.4. |
| Focused window (AX) | Screen now | `Evidence` | `extract_untrusted` at ≤ 0.4. |
| Screen OCR | Visual recall | `Evidence` | Same as capture. |
| Meeting transcript / recap | Session with provenance | `Evidence` today (`source=meeting`) | Recap persist gate is separate (#142). |
| Session / thread summary | Stored summary | `SessionSummary` / `ThreadSummary` | Already labeled unverified in the pack. |
| State table row | World model | `StateFact` | Confidence gate (FR-ST-20). |
| Learned lesson | User edit distill | `Lesson` | Content only; never L1/L2/L3. |

Mail that synced through Composio is still mail. It does not outrank AX just because the transport was MCP.

## Query-time merge

`score_block` still wins. When scores tie, `source_rank`:

`Structured` > session/thread summary > `StateFact` > `Evidence` > `Lesson`

This runs:

1. On the **raw** context pack (compression default-off): search hits with equal score keep calendar above AX / gmail.
2. On the **compress** path (`SHOGUN_COMPRESSION=1`): `evidence_to_blocks` tags `gcal` as `Structured` so the existing tie-break actually fires.

A missing connector does not abort the loop. Local evidence is the fallback.

## Write-time (the “possibly” hole)

Local extract maxes at 0.4 so heuristics never become facts. Calendar events used to take that path too, so “Standup” could become a low-confidence commitment.

Now: `Service::is_structured_fact` (calendar only) skips extract. The event stays searchable. Speech-act commitments still come from mail / capture.

Do **not** persist calendar titles as High-band commitments. A meeting is not a promise.

## Conflict

Same entity, two sources: higher `source_rank` / confidence owns the prompt slot; the other stays provenance. Auto-merge of people (`identity.rs`) is still not called from ingest — that is a follow-up, not this PR.

## Out of this slice

- `effective_confidence = trust_prior × extract × freshness` as a stored formula on every state row.
- User-facing priority settings.
- Raising Gmail-body extract to 0.9.
- Distilling lessons from capture.
