//! The generated meeting minutes (MT4, §6.16): prompt construction and model-output parsing.
//!
//! Pure, deterministic logic. `build_prompt` turns a transcript plus the user's notes into a
//! single instruction string; `parse_minutes` turns the model's reply back into a
//! [`MeetingMinutes`]. No network, no model call, no feature gate — the wiring layer (slice 2)
//! sends the prompt through the Select KK Batch lane and feeds the reply here.
//!
//! Invariant 4: the minutes never *do* anything. `next_actions` are suggestions to be confirmed,
//! and the prompt says so explicitly — a line like "email the vendor" is a proposal for the user,
//! never an action this system executes.

use std::fmt;

/// One extracted next action. `owner` is who the model thinks should do it, when the transcript
/// makes that clear; absent otherwise (we never invent an owner).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NextAction {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

/// The structured minutes: a prose summary, the decisions reached, and the suggested next actions.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct MeetingMinutes {
    pub summary: String,
    #[serde(default)]
    pub decisions: Vec<String>,
    #[serde(default)]
    pub next_actions: Vec<NextAction>,
}

impl MeetingMinutes {
    /// Serialize the two structured fields to the JSON strings the memory repo stores in its
    /// `decisions` / `next_actions` columns. Returns `(decisions_json, next_actions_json)`. The
    /// summary is passed to the repo separately (it is a plain column, not JSON).
    ///
    /// Serialization of `Vec<String>` / `Vec<NextAction>` cannot fail, so this returns the strings
    /// directly rather than a `Result`.
    pub fn to_columns(&self) -> (String, String) {
        // Vec<String> and Vec<NextAction> always serialize; the fallback keeps this infallible
        // without an `unwrap` on a value that cannot fail.
        let decisions = serde_json::to_string(&self.decisions).unwrap_or_else(|_| "[]".to_string());
        let next_actions =
            serde_json::to_string(&self.next_actions).unwrap_or_else(|_| "[]".to_string());
        (decisions, next_actions)
    }
}

/// One transcript line, as handed to [`build_prompt`]. `speaker` is `"me"` / `"other"` from the
/// capture source, or `None` when unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TranscriptLine<'a> {
    pub speaker: Option<&'a str>,
    pub text: &'a str,
}

/// The model output did not contain minutes we could parse. The caller keeps its degraded Recap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MinutesError {
    /// No JSON object was found in the output at all.
    NoJson,
    /// A JSON object was found but did not deserialize into [`MeetingMinutes`].
    Parse(String),
}

impl fmt::Display for MinutesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MinutesError::NoJson => write!(f, "no JSON object found in model output"),
            MinutesError::Parse(msg) => write!(f, "failed to parse minutes JSON: {msg}"),
        }
    }
}

impl std::error::Error for MinutesError {}

/// Build the instruction sent to the model.
///
/// The transcript is given line by line, each prefixed by its speaker (`me:` / `other:`, or
/// `speaker:` when unknown). The user's notes are appended when present. The model is asked to
/// return **only** a JSON object, written **in `lang`** (e.g. `"en"` — English is the base per §8),
/// and told plainly that next actions are suggestions to confirm, never actions to run (invariant 4).
pub fn build_prompt(lines: &[TranscriptLine], notes: Option<&str>, lang: &str) -> String {
    let mut p = String::new();

    p.push_str(
        "You are writing the minutes of a meeting from its transcript. Read the transcript and \
         the user's notes below, then produce a concise record of what was discussed.\n\n",
    );

    p.push_str("TRANSCRIPT:\n");
    if lines.is_empty() {
        p.push_str("(no transcript captured)\n");
    } else {
        for line in lines {
            let speaker = line.speaker.unwrap_or("speaker");
            p.push_str(speaker);
            p.push_str(": ");
            p.push_str(line.text);
            p.push('\n');
        }
    }
    p.push('\n');

    if let Some(notes) = notes.map(str::trim).filter(|n| !n.is_empty()) {
        p.push_str("USER NOTES (the user's own words — treat as authoritative):\n");
        p.push_str(notes);
        p.push_str("\n\n");
    }

    p.push_str(
        "Return ONLY a JSON object, with no prose and no code fence, in this exact shape:\n\
         {\"summary\": \"...\", \"decisions\": [\"...\"], \"next_actions\": [{\"text\": \"...\", \"owner\": \"...\" or null}]}\n\n",
    );

    p.push_str("Rules:\n");
    p.push_str(&format!(
        "- Write every string value in the language \"{lang}\".\n"
    ));
    p.push_str(
        "- \"decisions\" are conclusions the meeting actually reached; use an empty array if none.\n",
    );
    p.push_str(
        "- \"next_actions\" are SUGGESTIONS for the user to review and confirm, never actions to \
         execute. Do not send, post, schedule, or perform anything — only propose. Set \"owner\" \
         to the responsible person if the transcript makes it clear, otherwise null.\n",
    );

    p
}

/// Parse the model output into [`MeetingMinutes`].
///
/// Models often wrap the JSON in prose or ```json fences, so we extract the first `{` through the
/// matching last `}` and parse that slice. Tolerant by design: `decisions` and `next_actions`
/// default to empty, and `owner` may be absent or null. A missing or unparseable object is an
/// `Err`, and the caller keeps its degraded Recap rather than showing nothing.
pub fn parse_minutes(model_output: &str) -> Result<MeetingMinutes, MinutesError> {
    let start = model_output.find('{').ok_or(MinutesError::NoJson)?;
    let end = model_output.rfind('}').ok_or(MinutesError::NoJson)?;
    if end < start {
        return Err(MinutesError::NoJson);
    }
    let slice = &model_output[start..=end];
    serde_json::from_str(slice).map_err(|e| MinutesError::Parse(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_lines() -> Vec<TranscriptLine<'static>> {
        vec![
            TranscriptLine { speaker: Some("me"), text: "should we ship in Q3?" },
            TranscriptLine { speaker: Some("other"), text: "yes, Q3 works for us" },
            TranscriptLine { speaker: None, text: "agreed" },
        ]
    }

    #[test]
    fn prompt_includes_each_speaker_and_line() {
        let p = build_prompt(&sample_lines(), None, "en");
        assert!(p.contains("me: should we ship in Q3?"));
        assert!(p.contains("other: yes, Q3 works for us"));
        // Unknown speaker is labelled, never blank.
        assert!(p.contains("speaker: agreed"));
    }

    #[test]
    fn prompt_includes_the_notes_when_present() {
        let p = build_prompt(&sample_lines(), Some("- pricing still open"), "en");
        assert!(p.contains("USER NOTES"));
        assert!(p.contains("- pricing still open"));
    }

    #[test]
    fn prompt_omits_the_notes_section_when_absent_or_blank() {
        assert!(!build_prompt(&sample_lines(), None, "en").contains("USER NOTES"));
        assert!(!build_prompt(&sample_lines(), Some("   "), "en").contains("USER NOTES"));
    }

    #[test]
    fn prompt_names_the_output_language() {
        let p = build_prompt(&sample_lines(), None, "ja");
        assert!(p.contains("\"ja\""), "language code missing from prompt");
    }

    #[test]
    fn prompt_states_the_suggestion_and_confirm_invariant() {
        // Invariant 4: next actions are proposals, never executed.
        let p = build_prompt(&sample_lines(), None, "en");
        assert!(p.contains("SUGGESTIONS"));
        assert!(p.contains("confirm"));
        assert!(p.contains("never actions to execute"));
    }

    #[test]
    fn clean_json_parses_into_the_struct() {
        let out = r#"{"summary":"we agreed to ship","decisions":["ship in Q3"],"next_actions":[{"text":"tell the team","owner":"Alice"}]}"#;
        let m = parse_minutes(out).unwrap();
        assert_eq!(m.summary, "we agreed to ship");
        assert_eq!(m.decisions, vec!["ship in Q3"]);
        assert_eq!(m.next_actions, vec![NextAction { text: "tell the team".into(), owner: Some("Alice".into()) }]);
    }

    #[test]
    fn json_in_a_code_fence_parses() {
        let out = "Here are the minutes:\n```json\n{\"summary\":\"done\",\"decisions\":[],\"next_actions\":[]}\n```\nHope that helps!";
        let m = parse_minutes(out).unwrap();
        assert_eq!(m.summary, "done");
    }

    #[test]
    fn json_wrapped_in_prose_parses() {
        let out = "Sure! {\"summary\":\"ok\"} Let me know if you need more.";
        let m = parse_minutes(out).unwrap();
        assert_eq!(m.summary, "ok");
    }

    #[test]
    fn missing_decisions_and_next_actions_default_to_empty() {
        let m = parse_minutes(r#"{"summary":"just a summary"}"#).unwrap();
        assert!(m.decisions.is_empty());
        assert!(m.next_actions.is_empty());
    }

    #[test]
    fn owner_null_or_absent_becomes_none() {
        let m = parse_minutes(
            r#"{"summary":"s","next_actions":[{"text":"a","owner":null},{"text":"b"}]}"#,
        )
        .unwrap();
        assert_eq!(m.next_actions[0].owner, None);
        assert_eq!(m.next_actions[1].owner, None);
    }

    #[test]
    fn garbage_with_no_json_is_an_error() {
        assert_eq!(parse_minutes("no braces here at all"), Err(MinutesError::NoJson));
    }

    #[test]
    fn a_broken_json_object_is_a_parse_error() {
        // A brace pair exists but the contents are not valid minutes.
        let err = parse_minutes("{not valid json}").unwrap_err();
        assert!(matches!(err, MinutesError::Parse(_)));
    }

    #[test]
    fn round_trips_through_serde() {
        let original = MeetingMinutes {
            summary: "agreed on the plan".into(),
            decisions: vec!["ship in Q3".into(), "hire one engineer".into()],
            next_actions: vec![
                NextAction { text: "draft the JD".into(), owner: Some("Bob".into()) },
                NextAction { text: "book the room".into(), owner: None },
            ],
        };
        let json = serde_json::to_string(&original).unwrap();
        let recovered = parse_minutes(&json).unwrap();
        assert_eq!(recovered, original);
    }

    #[test]
    fn to_columns_serializes_only_the_json_fields() {
        let m = MeetingMinutes {
            summary: "not included here".into(),
            decisions: vec!["d1".into()],
            next_actions: vec![NextAction { text: "a1".into(), owner: None }],
        };
        let (decisions, next_actions) = m.to_columns();
        assert_eq!(decisions, r#"["d1"]"#);
        // owner is None and skipped, not serialized as null.
        assert_eq!(next_actions, r#"[{"text":"a1"}]"#);
    }
}
