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
- Fixed prompt-boundary injection risk by escaping `<`, `>`, and `&` in every untrusted app, field, visible-text, and memory value before serialization inside the context block.
- Added adversarial coverage placing the closing delimiter in app, field label, text before/after the cursor, and memory; only the real closing marker remains in the generated prompt.
- Updated desktop capture preassembly to fingerprint captured AX text per focus key. Same-window content changes rebuild `ReplyContext`; unchanged content skips rebuild. Option release still performs no retrieval.
- Added desktop helper tests proving same-key changed content refreshes and same-key unchanged content does not.
- Added `ReplyContextCache::clear()` and centralized capture invalidation. Excluded, empty, unavailable, no-window, app-self, Accessibility-untrusted, and policy-lock-failure paths now clear the warm pack and reset fingerprint state, preventing prior sensitive content from reaching BYOK.
- Added cache-clear and desktop invalidation tests for excluded, empty, and unavailable outcomes.

## Validation

- `cargo test -p shogun-core inline::tests`: 14 passed (including delimiter adversary).
- `cargo test -p shogun-core`: 319 passed after cache invalidation change.
- `cargo check -p shogun-desktop-spike`: passed after cache invalidation change.
- `git diff --check`: passed.

The Rust commands emit existing unrelated warnings (`unused_unsafe`, unused imports, dead code, and naming warnings); no new warning was introduced by this change.

An initial package-name probe used `cargo check -p shogun-desktop`, which correctly failed because the package is named `shogun-desktop-spike`; the required check then passed with the correct package name.

## Scope and preservation

- No schema, migration, React data logic, screenshot upload, OCR/network retrieval, or trigger change.
- Existing untracked `.pnpm-store/` and `apps/desktop/src/usePointerMove.ts` remain untouched.

## Review notes

Classifier matching is intentionally small and deterministic. Unknown surfaces do not force formality; visible text remains the tone source. Context is still bounded by the existing `ReplyContext`/memory limits, and the Option release path performs no new broad retrieval.

## Follow-up audit fix

The original fixed XML-like delimiter was unsafe because captured text could contain the literal closing marker. The boundary writer now encodes angle brackets and ampersands as `\\u003c`, `\\u003e`, and `\\u0026`, so captured content cannot terminate the block. The capture poller also no longer treats a stable window key as stable content: it hashes each captured AX text and rebuilds only on a changed hash or focus key.
