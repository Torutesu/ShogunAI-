//! Inline draft composition — the at-cursor generation (§6.6, SLO-03). The core of "press the
//! Option key → the best continuation appears at your caret":
//!
//!   read the field around the cursor + relevant memory  →  build a prompt
//!     →  generate on the BYOK Agent lane  →  record the egress trace  →  insert at the caret.
//!
//! Two device seams keep the whole flow Linux-testable without a Mac or a network:
//! - [`CursorReader`] reads the focused field's surrounding text (Accessibility API on device).
//! - [`TextInserter`] writes the generated text at the caret (Accessibility API on device).
//!
//! Invariants held here:
//! - **Key separation (5):** generation goes through the [`AgentClient`] — the BYOK Agent lane, never
//!   the Batch lane. There is no way to reach this with a Select KK key (the type won't construct one).
//! - **No L1 send (4):** inserting at the caret is a device-local write — it never leaves the device,
//!   so there is no L3 send in this flow. The user sends from their own app, if they choose to.
//! - **Traceability (3) / no captured text in logs (G8):** the prompt that egresses (the surrounding
//!   text + memory) records exactly one [`TraceRecord`] — digest + byte length only, never the text.
//! - **AX text only (2):** the context is text read from the field; no screenshot is ever involved.

use crate::llm::traceability::{Route, TraceRecord, TraceabilitySink};
use crate::llm::AgentClient;

/// The text around the caret in the focused field, plus which app/field it is. AX text only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorContext {
    /// The focused app (bundle id or name) — grounds the tone.
    pub app: String,
    /// A short label for the field/window (e.g. an email subject); may be empty.
    pub field_label: String,
    /// The text before the caret.
    pub before: String,
    /// The text after the caret (often empty when composing at the end of a field).
    pub after: String,
}

impl CursorContext {
    /// Nothing to work with — an empty field with no label.
    pub fn is_empty(&self) -> bool {
        self.before.trim().is_empty() && self.after.trim().is_empty() && self.field_label.trim().is_empty()
    }
}

/// Reads the focused field around the caret. The device impl uses `AXFocusedUIElement` + `AXValue`
/// + `AXSelectedTextRange`; tests inject a fake. `None` when no editable field is focused.
pub trait CursorReader {
    fn read(&self) -> Option<CursorContext>;
}

/// Inserts text at the caret in the focused field. The device impl sets `AXSelectedText`; tests
/// inject a fake. Returns a non-sensitive error string on failure (never the field text).
pub trait TextInserter {
    fn insert(&self, text: &str) -> Result<(), String>;
}

/// The result of an inline generate-and-insert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineOutcome {
    /// Generated and inserted at the caret; carries the inserted char count.
    Inserted { chars: usize },
    /// No editable field / empty context — nothing to do (no egress, no trace).
    NoContext,
    /// Generation failed on the Agent lane — nothing egressed a usable result, nothing inserted.
    GenerationFailed(String),
    /// Generation succeeded (and was traced) but writing at the caret failed.
    InsertFailed(String),
}

/// Build the Agent-lane prompt from the caret context + relevant memory (already confidence-gated by
/// the caller — FR-ST-20). Pure: this is the piece that turns "what's on screen + what I remember"
/// into an instruction to write the best continuation, asking for *only* the text to insert.
pub fn build_prompt(ctx: &CursorContext, memory: &[String]) -> String {
    let mut p = String::new();
    p.push_str("You are writing directly in the user's active app");
    if !ctx.app.trim().is_empty() {
        p.push_str(" (");
        p.push_str(ctx.app.trim());
        if !ctx.field_label.trim().is_empty() {
            p.push_str(" — ");
            p.push_str(ctx.field_label.trim());
        }
        p.push(')');
    }
    p.push_str(". Continue the text at the cursor in the user's own voice. ");
    p.push_str("Output only the text to insert at the cursor — no preamble, no quotation marks, no sign-off unless the context clearly calls for one.\n");

    let facts: Vec<&str> = memory.iter().map(|m| m.trim()).filter(|m| !m.is_empty()).collect();
    if !facts.is_empty() {
        p.push_str("\nWhat the user has in view and remembers:\n");
        for m in facts {
            p.push_str("- ");
            p.push_str(m);
            p.push('\n');
        }
    }

    p.push_str("\nText before the cursor:\n");
    p.push_str(&ctx.before);
    if !ctx.after.trim().is_empty() {
        p.push_str("\n\nText after the cursor:\n");
        p.push_str(&ctx.after);
    }
    p
}

/// The full composition: read the caret context, and if there is one, build the prompt, generate on
/// the BYOK Agent lane, record the egress trace, then insert at the caret. Any failing step stops
/// the flow without inserting. `destination` labels the trace (e.g. the model host).
///
/// Ordering matters for invariant 3: the trace is recorded **after** a successful `complete` (the
/// chunk has provably egressed) and **before** the insert — so a chunk that reached the wire always
/// leaves a trace, even if the local insert then fails.
pub fn compose_inline<R, A, I, S>(
    reader: &R,
    agent: &A,
    inserter: &I,
    sink: &S,
    memory: &[String],
    destination: &str,
) -> InlineOutcome
where
    R: CursorReader + ?Sized,
    A: AgentClient + ?Sized,
    I: TextInserter + ?Sized,
    S: TraceabilitySink + ?Sized,
{
    let Some(ctx) = reader.read() else {
        return InlineOutcome::NoContext;
    };
    if ctx.is_empty() {
        return InlineOutcome::NoContext;
    }
    let prompt = build_prompt(&ctx, memory);
    let text = match agent.complete(&prompt) {
        Ok(t) => t,
        Err(e) => return InlineOutcome::GenerationFailed(e.to_string()),
    };
    sink.record(TraceRecord::for_chunk(Route::MessagesApi, "inline_draft", destination, &prompt, false));
    match inserter.insert(&text) {
        Ok(()) => InlineOutcome::Inserted { chars: text.chars().count() },
        Err(e) => InlineOutcome::InsertFailed(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::traceability::{digest, RecordingSink};
    use crate::llm::{AgentClient, ByokKey, LlmError, MockAgentClient, Secret};

    fn ctx() -> CursorContext {
        CursorContext {
            app: "Mail".into(),
            field_label: "Re: Q3 roadmap".into(),
            before: "Hi Alice,\n\n".into(),
            after: String::new(),
        }
    }
    struct FixedReader(Option<CursorContext>);
    impl CursorReader for FixedReader {
        fn read(&self) -> Option<CursorContext> {
            self.0.clone()
        }
    }
    struct FakeInserter {
        ok: bool,
        last: std::cell::RefCell<String>,
    }
    impl TextInserter for FakeInserter {
        fn insert(&self, text: &str) -> Result<(), String> {
            if self.ok {
                *self.last.borrow_mut() = text.to_string();
                Ok(())
            } else {
                Err("no focused field".into())
            }
        }
    }
    struct FailAgent;
    impl AgentClient for FailAgent {
        fn complete(&self, _p: &str) -> Result<String, LlmError> {
            Err(LlmError::NotConfigured)
        }
    }
    fn agent() -> MockAgentClient {
        MockAgentClient::new(ByokKey::new(Secret::new("byok-key")))
    }
    fn inserter(ok: bool) -> FakeInserter {
        FakeInserter { ok, last: std::cell::RefCell::new(String::new()) }
    }

    #[test]
    fn prompt_includes_field_memory_and_surrounding_text() {
        let p = build_prompt(&ctx(), &["you owe Alice the deck (Fri)".into(), "legal sign-off pending".into()]);
        assert!(p.contains("Mail — Re: Q3 roadmap"), "app + field label grounds the prompt: {p}");
        assert!(p.contains("Output only the text to insert"), "asks for insertion text only");
        assert!(p.contains("- you owe Alice the deck (Fri)"), "memory facts are included");
        assert!(p.contains("Hi Alice,"), "the text before the cursor is included");
    }

    #[test]
    fn empty_memory_omits_the_memory_section() {
        let p = build_prompt(&ctx(), &[]);
        assert!(!p.contains("What the user has in view"), "no memory ⇒ no memory section");
    }

    #[test]
    fn after_text_is_included_only_when_present() {
        let mut c = ctx();
        c.after = "Best,\nJordan".into();
        let p = build_prompt(&c, &[]);
        assert!(p.contains("Text after the cursor:\nBest,\nJordan"));
    }

    #[test]
    fn happy_path_inserts_and_records_one_digest_only_trace() {
        let sink = RecordingSink::new();
        let ins = inserter(true);
        let out = compose_inline(&FixedReader(Some(ctx())), &agent(), &ins, &sink, &["memo".into()], "api.anthropic.com");
        // the mock echoes "draft: <prompt>", which is what gets inserted
        assert!(matches!(out, InlineOutcome::Inserted { chars } if chars > 0));
        assert!(ins.last.borrow().starts_with("draft: "), "generated text was inserted at the caret");
        // exactly one egress trace, on the BYOK Agent lane (MessagesApi), digest of the prompt only
        let recs = sink.records();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].route, Route::MessagesApi);
        assert_eq!(recs[0].purpose, "inline_draft");
        let prompt = build_prompt(&ctx(), &["memo".into()]);
        assert_eq!(recs[0].chunk_xxh64, digest(&prompt));
        assert_eq!(recs[0].chunk_bytes, prompt.len());
    }

    #[test]
    fn trace_never_contains_the_field_text() {
        let sink = RecordingSink::new();
        let mut c = ctx();
        c.before = "SECRET-DIARY-ENTRY do not leak".into();
        let out = compose_inline(&FixedReader(Some(c)), &agent(), &inserter(true), &sink, &[], "host");
        assert!(matches!(out, InlineOutcome::Inserted { .. }));
        let dumped = format!("{:?}", sink.records()[0]);
        assert!(!dumped.contains("SECRET-DIARY-ENTRY"), "the field text must never reach the trace record");
    }

    #[test]
    fn no_field_focused_does_nothing() {
        let sink = RecordingSink::new();
        let out = compose_inline(&FixedReader(None), &agent(), &inserter(true), &sink, &[], "host");
        assert_eq!(out, InlineOutcome::NoContext);
        assert!(sink.records().is_empty(), "no egress when there's no field");
    }

    #[test]
    fn empty_context_does_nothing() {
        let empty = CursorContext { app: String::new(), field_label: String::new(), before: "   ".into(), after: String::new() };
        let sink = RecordingSink::new();
        let out = compose_inline(&FixedReader(Some(empty)), &agent(), &inserter(true), &sink, &[], "host");
        assert_eq!(out, InlineOutcome::NoContext);
        assert!(sink.records().is_empty());
    }

    #[test]
    fn generation_failure_inserts_nothing_and_traces_nothing() {
        let sink = RecordingSink::new();
        let ins = inserter(true);
        let out = compose_inline(&FixedReader(Some(ctx())), &FailAgent, &ins, &sink, &[], "host");
        assert!(matches!(out, InlineOutcome::GenerationFailed(_)));
        assert!(sink.records().is_empty(), "a failed generation egressed no usable chunk to trace");
        assert!(ins.last.borrow().is_empty(), "nothing was inserted");
    }

    #[test]
    fn insert_failure_still_traces_the_egress() {
        // generation succeeded (the chunk reached the wire) but the caret write failed → trace stands.
        let sink = RecordingSink::new();
        let out = compose_inline(&FixedReader(Some(ctx())), &agent(), &inserter(false), &sink, &[], "host");
        assert!(matches!(out, InlineOutcome::InsertFailed(_)));
        assert_eq!(sink.records().len(), 1, "the egress that reached the wire is still traced");
    }
}
