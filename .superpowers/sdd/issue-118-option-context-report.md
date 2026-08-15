# Issue 118 implementation report

## Status

Implemented instant app-aware Option-key drafting in `crates/shogun-core/src/inline.rs`.

## Changes

- Added pure `classify_surface(app, field_label)` classifier with tested styles for consumer messaging, email, team chat, documents/editors, and unknown surfaces.
- Updated inline prompt generation to derive style from active app/field metadata.
- Kept current visible text and preassembled reply context ahead of confidence-gated memory in the prompt.
- Delimited all captured app metadata, field labels, visible text, and context lines as `<untrusted_captured_context>` and explicitly rejected prompt-like instructions inside that block.
- Added explicit grounding rules: current/thread evidence first, memory only when it supports evidence, and no invented recipients, facts, commitments, names, or links.
- Preserved insertion-only output rules: no preamble, quotation marks, analysis, or meta text; response inserts verbatim.
- Left Option watcher semantics, immediate `drafting` acknowledgement, warm cache path, bounded fallback, and local AX-only capture unchanged.
- Added focused classifier and prompt-contract tests.

## Validation

- `cargo test -p shogun-core inline::tests`: 13 passed.
- `cargo test -p shogun-core`: 318 passed.
- `cargo check -p shogun-desktop-spike`: passed.
- `git diff --check`: passed.

The Rust commands emit existing unrelated warnings (`unused_unsafe`, unused imports, dead code, and naming warnings); no new warning was introduced by this change.

An initial package-name probe used `cargo check -p shogun-desktop`, which correctly failed because the package is named `shogun-desktop-spike`; the required check then passed with the correct package name.

## Scope and preservation

- No schema, migration, React data logic, screenshot upload, OCR/network retrieval, or trigger change.
- Existing untracked `.pnpm-store/` and `apps/desktop/src/usePointerMove.ts` remain untouched.

## Review notes

Classifier matching is intentionally small and deterministic. Unknown surfaces do not force formality; visible text remains the tone source. Context is still bounded by the existing `ReplyContext`/memory limits, and the Option release path performs no new broad retrieval.
