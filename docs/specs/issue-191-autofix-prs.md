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

- **Same repository** + mechanical class + non-empty diff → draft PR, title starts with `DO NOT MERGE:`. Base is the failing head branch so the draft is only the mechanical commit (not the original PR’s whole diff against `main`).
- **Fork PR** or no write to the head repo → comment only (classification + suggested command), including when the class is mechanically fixable. Never push to a fork.
- **Push to `main`** + mechanical class → draft PR targeting `main`.
- **Push to `main`** + non-mechanical → no issue spam; the failed run stays the record.
- Manual `workflow_dispatch` must name a failed `ci` run (decimal run id). Other workflows and successful runs are refused.

## Idempotency

Fingerprint = `class` + head SHA. If an open draft already carries label `ci-autofix` and that fingerprint in the body, do not open another. A leftover `autofix/…` branch without a PR is not treated as done — create the draft.

Classification uses **failed-job** logs and failure diagnostics (`clippy::`, `Would reformat:`, a guard script plus `violation`). Successful `cargo clippy` / `check-*.py` command lines in the aggregate log must not pick the class. `macos-14` / `context cache` / `idle cpu` are not class signals — they appear on healthy jobs. `shogun-desktop-spike` clippy is `macos` (comment-only): the workspace apply excludes that crate. `ci` does not run `cargo fmt --check` today; the rustfmt class stays reserved for an actual rustfmt diagnostic so a format gate is not bolted onto an unformatted tree.

## Privacy

`scripts/ci_autofix.py redact` runs on the complete raw log before any GitHub write. It strips the same issuer prefixes as `shogun-redact` (`ghp_`/`gho_`/`ghu_`/`ghs_`/`ghr_`, Slack `xox*`, AWS `AKIA`/`ASIA`, Google `AIza`/`ya29.`, …), bearer tokens including `+/=`, PEM blocks, and licence keys of the shape `shogun-XXXX-XXXX-XXXX-XXXX` (Crockford). Crate names such as `shogun-desktop-spike` are not licence keys. Classifier runs on the redacted log.

Jobs are split so a write-capable token never sits in the same job as `cargo`:

1. **classify** — read token; redact; classify.
2. **apply** — read token only to fetch the SHA; `GITHUB_TOKEN`/`GH_TOKEN` are emptied before cargo. Upload a `*.rs`-only patch or refuse.
3. **publish** — write token; `git apply` the patch (no rustc); draft PR + `ci-autofix` label.
4. **comment** — write token; PR lookup is `GET /commits/{sha}/pulls`, not a text search.

`diff-guard` refuses any apply that touches a non-`.rs` path (workflows, lockfiles, scripts). `clippy --fix -- -D warnings` may exit 1 when unfixable lints remain; a non-empty Rust-only diff still publishes.

## Done when

- `python3 scripts/ci_autofix.py --self-test` is in the `ci` invariant-guard step.
- A failed same-repo `ci` run can produce at most one draft PR (mechanical) or one comment (otherwise).
- Nothing in this workflow merges.
