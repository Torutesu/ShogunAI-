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

Search-hit confidence is no longer a constant `1.0`. Fusion applies the issue formula at
query time (not as a stored column on every state row):

```
effective_confidence = source_trust_prior × extraction_confidence × freshness_decay
```

| Block | `SourceKind` | prior | extract | decay (this slice) | confidence |
|---|---|---|---|---|---|
| Calendar title / time / event id | `Structured` | 0.9 | 1.0 (extractor bypass) | 1.0 | 0.9 |
| Calendar description, AX, mail, chat | `Evidence` | 0.4 | 1.0 (quoted, not extracted) | 1.0 | 0.4 |

Quoted evidence uses extract `1.0` so we do not double-apply the 0.4 local-rule cap. The prior
is what stops notes from looking like verified facts. Age curves (calendar until next sync,
AX in seconds) stay `ScoreInputs.freshness` so freshness is not multiplied twice.

`score_block` still wins. When scores tie, `source_rank`:

`Structured` > session/thread summary > `StateFact` > `Evidence` > `Lesson`

This runs:

1. On the **raw** context pack (compression default-off): equal FTS scores keep calendar above AX / gmail via `source_rank` on the event `source` column.
2. On the **compress** path (`SHOGUN_COMPRESSION=1`): `evidence_to_blocks` tags calendar *metadata* as `Structured` (higher confidence **and** rank). The description is a second `Evidence` block.

A missing connector does not abort the loop. Local evidence is the fallback.

## Write-time (the “possibly” hole)

Local extract maxes at 0.4 so heuristics never become facts. Calendar events used to take that path too, so “Standup” could become a low-confidence commitment.

Now: `Service::is_structured_fact` (calendar only) skips extract. The event stays searchable. Speech-act commitments still come from mail / capture.

Do **not** persist calendar titles as High-band commitments. A meeting is not a promise.

## Conflict

Same entity, two sources: higher `source_rank` / confidence owns the prompt slot; the other stays provenance. Auto-merge of people (`identity.rs`) is still not called from ingest — that is a follow-up, not this PR.

## Out of this slice

- Persisting `effective_confidence` on every state row (query-time only, this PR).
- Per-source freshness *curves* from `last_sync` / event age (decay is `1.0` until then).
- User-facing priority settings.
- Raising Gmail-body extract to 0.9.
- Distilling lessons from capture.
