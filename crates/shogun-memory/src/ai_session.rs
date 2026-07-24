//! Ingesting the user's sessions with AI coding tools as first-class memory.
//!
//! A large share of knowledge work now happens *inside* an AI tool: what was decided, what was
//! tried, what broke. Scraping that off the screen loses almost everything — the visible slice
//! only, no turn structure, no timestamps, and the same text re-captured on every scroll. The
//! tools keep structured local session logs instead, which carry role, time and a session id, so
//! reading those is both higher fidelity and cheaper.
//!
//! This module is the **pure** half: one JSONL line in, an optional [`SessionTurn`] out. Walking
//! the directory and writing rows lives in the effectful layer.
//!
//! What is deliberately dropped:
//!
//! * **tool calls and their results** — file contents and command output dominate the log by
//!   volume and are noise as memory; the conversation is the signal.
//! * **model reasoning blocks** — not something the user said or was told.
//! * **images** — never stored at all (invariant 2: text only).

/// Who produced a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

/// One conversational turn recovered from a session log.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionTurn {
    /// The tool's own session identifier — the natural thread key.
    pub session_id: String,
    pub role: Role,
    /// Unix ms, parsed from the log's RFC3339 timestamp.
    pub ts_ms: i64,
    pub text: String,
    /// The working directory the session ran in, when recorded — a strong project hint.
    pub cwd: Option<String>,
}

/// Parse one JSONL line from a Claude Code session log.
///
/// Returns `None` for anything that is not a user/assistant turn with text: tool traffic,
/// reasoning, images, metadata lines, and malformed JSON are all skipped rather than erroring —
/// a session log is an append-only stream that may be mid-write, and one bad line must not stop
/// the import.
pub fn parse_claude_code_line(line: &str) -> Option<SessionTurn> {
    let v: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    let role = match v.get("type")?.as_str()? {
        "user" => Role::User,
        "assistant" => Role::Assistant,
        _ => return None,
    };
    let session_id = v.get("sessionId")?.as_str()?.to_string();
    let ts_ms = v.get("timestamp").and_then(|t| t.as_str()).and_then(parse_rfc3339_ms)?;
    let text = extract_text(v.get("message")?.get("content")?);
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    Some(SessionTurn {
        session_id,
        role,
        ts_ms,
        text: text.to_string(),
        cwd: v.get("cwd").and_then(|c| c.as_str()).map(str::to_string),
    })
}

/// Pull the human-readable text out of a message `content`, which is either a bare string or an
/// array of typed blocks. Only `text` blocks are taken — see the module note on what is dropped.
fn extract_text(content: &serde_json::Value) -> String {
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    let Some(blocks) = content.as_array() else { return String::new() };
    let mut parts: Vec<&str> = Vec::new();
    for b in blocks {
        if b.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(s) = b.get("text").and_then(|t| t.as_str()) {
                parts.push(s);
            }
        }
    }
    parts.join("\n")
}

/// Parse the RFC3339 timestamps these logs use into unix ms.
///
/// Hand-rolled rather than pulling in a date library: the format is fixed
/// (`YYYY-MM-DDTHH:MM:SS[.fff]Z`), and the arithmetic is a days-since-epoch computation that is
/// easier to test than to justify a dependency for.
fn parse_rfc3339_ms(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' {
        return None;
    }
    let num = |from: usize, to: usize| -> Option<i64> { s.get(from..to)?.parse::<i64>().ok() };
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, sec) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    // Fractional seconds, when present.
    let millis = if b.get(19) == Some(&b'.') {
        let frac: String = s[20..].chars().take_while(char::is_ascii_digit).collect();
        let mut ms = frac.chars().take(3).collect::<String>();
        while ms.len() < 3 {
            ms.push('0');
        }
        ms.parse::<i64>().unwrap_or(0)
    } else {
        0
    };
    Some((days_from_civil(y, mo, d) * 86_400 + h * 3_600 + mi * 60 + sec) * 1_000 + millis)
}

/// Days since 1970-01-01 for a proleptic-Gregorian date (Howard Hinnant's `days_from_civil`).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_and_known_dates_convert_correctly() {
        assert_eq!(parse_rfc3339_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339_ms("2000-03-01T00:00:00Z"), Some(951_868_800_000));
        // A leap day, which a naive month-length table gets wrong.
        assert_eq!(parse_rfc3339_ms("2024-02-29T12:00:00Z"), Some(1_709_208_000_000));
    }

    #[test]
    fn fractional_seconds_are_kept_as_milliseconds() {
        assert_eq!(parse_rfc3339_ms("1970-01-01T00:00:00.250Z"), Some(250));
        // More precision than milliseconds is truncated, not rounded up into the next second.
        assert_eq!(parse_rfc3339_ms("1970-01-01T00:00:00.999999Z"), Some(999));
        // Fewer digits are padded, not misread as milliseconds.
        assert_eq!(parse_rfc3339_ms("1970-01-01T00:00:00.5Z"), Some(500));
    }

    #[test]
    fn malformed_timestamps_are_rejected() {
        assert_eq!(parse_rfc3339_ms("not a date"), None);
        assert_eq!(parse_rfc3339_ms("1970-13-01T00:00:00Z"), None);
        assert_eq!(parse_rfc3339_ms(""), None);
    }

    #[test]
    fn a_user_turn_with_plain_string_content_is_recovered() {
        let line = r#"{"type":"user","sessionId":"abc","timestamp":"2026-07-25T10:00:00.000Z",
                       "cwd":"/home/me/proj","message":{"role":"user","content":"why is the build red?"}}"#;
        let t = parse_claude_code_line(line).expect("parsed");
        assert_eq!(t.role, Role::User);
        assert_eq!(t.session_id, "abc");
        assert_eq!(t.text, "why is the build red?");
        assert_eq!(t.cwd.as_deref(), Some("/home/me/proj"));
    }

    #[test]
    fn an_assistant_turn_takes_only_its_text_blocks() {
        let line = r#"{"type":"assistant","sessionId":"abc","timestamp":"2026-07-25T10:00:01Z",
            "message":{"role":"assistant","content":[
              {"type":"thinking","thinking":"internal reasoning that is not a turn"},
              {"type":"text","text":"The migration is missing."},
              {"type":"tool_use","name":"Bash","input":{"command":"cargo test"}},
              {"type":"text","text":"Adding it now."}
            ]}}"#;
        let t = parse_claude_code_line(line).expect("parsed");
        assert_eq!(t.role, Role::Assistant);
        assert_eq!(t.text, "The migration is missing.\nAdding it now.");
        assert!(!t.text.contains("internal reasoning"), "reasoning is not a turn");
        assert!(!t.text.contains("cargo test"), "tool calls are noise, not memory");
    }

    #[test]
    fn tool_results_and_images_yield_nothing() {
        // A user line that is only a tool result — by volume the most common line in a log.
        let tool = r#"{"type":"user","sessionId":"abc","timestamp":"2026-07-25T10:00:02Z",
            "message":{"role":"user","content":[{"type":"tool_result","content":"file contents…"}]}}"#;
        assert_eq!(parse_claude_code_line(tool), None);

        let image = r#"{"type":"user","sessionId":"abc","timestamp":"2026-07-25T10:00:03Z",
            "message":{"role":"user","content":[{"type":"image","source":{"data":"BASE64"}}]}}"#;
        assert_eq!(parse_claude_code_line(image), None, "invariant 2: no image data is stored");
    }

    #[test]
    fn non_conversation_lines_are_skipped() {
        for line in [
            r#"{"type":"system","sessionId":"abc","timestamp":"2026-07-25T10:00:00Z"}"#,
            r#"{"type":"queue-operation","sessionId":"abc"}"#,
            r#"{"type":"mode","sessionId":"abc"}"#,
        ] {
            assert_eq!(parse_claude_code_line(line), None, "{line}");
        }
    }

    #[test]
    fn a_truncated_or_malformed_line_does_not_stop_the_import() {
        assert_eq!(parse_claude_code_line(r#"{"type":"user","sessionI"#), None);
        assert_eq!(parse_claude_code_line(""), None);
        assert_eq!(parse_claude_code_line("   "), None);
        // Well-formed JSON but missing the fields we need.
        assert_eq!(parse_claude_code_line(r#"{"type":"user"}"#), None);
    }

    /// Run against a real session log to check the parser against the actual format rather than
    /// against fixtures written from the same assumptions:
    /// `SHOGUN_AI_SESSION_LOG=~/.claude/projects/…/x.jsonl cargo test -p shogun-memory -- --ignored`
    /// Ignored by default — CI has no session logs, and this must never depend on a real user's.
    #[test]
    #[ignore = "needs a real session log; set SHOGUN_AI_SESSION_LOG"]
    fn parses_a_real_session_log() {
        let Ok(path) = std::env::var("SHOGUN_AI_SESSION_LOG") else { return };
        let text = std::fs::read_to_string(&path).expect("read the log");
        let (mut turns, mut users, mut assistants) = (0usize, 0usize, 0usize);
        let mut sessions = std::collections::HashSet::new();
        for line in text.lines() {
            if let Some(t) = parse_claude_code_line(line) {
                turns += 1;
                match t.role {
                    Role::User => users += 1,
                    Role::Assistant => assistants += 1,
                }
                assert!(t.ts_ms > 1_500_000_000_000, "a real timestamp, not a parse artefact");
                assert!(!t.text.trim().is_empty());
                sessions.insert(t.session_id);
            }
        }
        eprintln!(
            "parsed {turns} turns ({users} user / {assistants} assistant) across {} session(s) from {} lines",
            sessions.len(),
            text.lines().count()
        );
        assert!(turns > 0, "a real log must yield turns");
        assert!(users > 0 && assistants > 0, "both sides of the conversation must be recovered");
    }

    #[test]
    fn whitespace_only_turns_are_dropped() {
        let line = r#"{"type":"user","sessionId":"abc","timestamp":"2026-07-25T10:00:00Z",
                       "message":{"role":"user","content":"   \n  "}}"#;
        assert_eq!(parse_claude_code_line(line), None);
    }
}
