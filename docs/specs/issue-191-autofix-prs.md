# Issue #191 — detect errors, open fix PRs (never merge them)

**Status:** v1 slice on `feat/ci-autofix-draft-prs`  
**Assigned:** Anand (`Anandb71`)  
**Adjacent, not this PR:** Toru [#236](https://github.com/Torutesu/ShogunAI-/pull/236) / [#239](https://github.com/Torutesu/ShogunAI-/issues/239) Help & Support *intake*. Mikel meeting UI. [#108](https://github.com/Torutesu/ShogunAI-/issues/108) is the CS routing sibling — this slice is the *CI → draft PR* half.

## Why

A failed `ci` run today dies in Actions with no branch, no classified cause, and no mechanical patch. Humans grep logs. That does not scale into launch week.

This is the same discipline as the product loop: **estimate state, then act under a permission gate.** CI failure is the event. Classification is the world model. A draft PR is L3 (human merge). Auto-merge would be L1 send. We do not L1-send to `main`.

## Non-goals (do not “go all out” into these)

- LLM rewriting production crates unattended (no model on the merge path).
- Auto-merge, force-push, or writing to someone else’s fork.
- Opening GitHub **issues** from the bot (repo rule: no issue mutation unless a human says yes).
- Pasting capture text, Keychain material, license keys, or raw CI secrets into a PR body.
- Replacing Toru’s in-app Help & Support form.
- H2 OSS model routing (#103). Measure first; this workflow only *labels* harness/SLO failures.

## Trust classes

| Class | May mechanically apply | What the bot does |
|-------|------------------------|-------------------|
| `rustfmt` | `cargo fmt` | Draft PR with the fmt diff |
| `clippy` | `cargo clippy --fix` (workspace, exclude `shogun-desktop-spike`) | Draft PR with the fix diff |
| `invariant_guard` | nothing | Comment on the failing PR (same-repo). Guards must not be auto-silenced |
| `leak` | nothing | Comment *without* log excerpt (possible secret in the log) |
| `rust_test` / `frontend` / `api` / `macos` / `harness` / `unknown` | nothing | Comment on the failing PR with a **redacted** excerpt |

`harness` is #103-adjacent: SLO / `spike-harness` / context-cache budget failures get that class so they are not lumped into “random test.” Still no router.

## Same-repo vs fork

- **Same repository** + mechanical class + non-empty diff → draft PR, title starts with `DO NOT MERGE:`.
- **Fork PR** or no write to the head repo → comment only (classification + suggested command). Never push to a fork.
- **Push to `main`** + mechanical class → draft PR targeting `main`.
- **Push to `main`** + non-mechanical → no issue spam; the failed run stays the record.

## Idempotency

Fingerprint = `class` + head SHA. If an open draft already carries label `ci-autofix` and that fingerprint in the body, do not open another.

## Privacy

`scripts/ci_autofix.py redact` strips bearer tokens, `ghp_`/`gho_` prefixes, `sk-` keys, PEM blocks, and `shogun-` license-shaped strings before any GitHub write. Classifier runs on the redacted log.

## Done when

- `python3 scripts/ci_autofix.py --self-test` is in the `ci` invariant-guard step.
- A failed same-repo `ci` run can produce at most one draft PR (mechanical) or one comment (otherwise).
- Nothing in this workflow merges.
