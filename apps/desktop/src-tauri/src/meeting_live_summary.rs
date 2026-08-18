//! Rolling Live Summary for AI Canvas during an active meeting.
//!
//! Uses Select KK Messages (same lane as live translate) — not Batch (too slow for mid-call)
//! and not a heuristic paste of the transcript. The FE only requests a refresh after enough
//! spoken context has landed; this module refuses thin payloads and rate-limits in-flight work.

#![cfg(target_os = "macos")]

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use shogun_core::daemon::Db;
use shogun_core::llm::anthropic::{AnthropicConfig, AnthropicSelectKkMessagesClient};
use shogun_core::llm::transport::ReqwestTransport;
use shogun_core::llm::{Secret, SelectKkKey};
use tauri::{Emitter, Manager};

use shogun_integrations::keychain_store;

const SUMMARY_MODEL: &str = "claude-haiku-4-5-20251001";
const SUMMARY_PURPOSE: &str = "meeting_live_summary";
/// Refuse to call the model until the transcript has real substance.
const MIN_CHARS: usize = 420;
/// Soft ceiling so a long meeting does not dump the whole hour every refresh.
const MAX_CHARS: usize = 6_000;
/// Minimum gap between successful (or in-flight) summary jobs.
const MIN_INTERVAL_MS: i64 = 45_000;

static IN_FLIGHT: AtomicBool = AtomicBool::new(false);
static LAST_STARTED_MS: AtomicI64 = AtomicI64::new(0);

const SYSTEM: &str = "You summarize an ongoing meeting for a live overlay.\n\
- Write 2–5 short sentences (or up to 6 tight bullets) covering what has been discussed so far.\n\
- Capture topics, decisions, open questions, and owners when clear.\n\
- Do NOT quote the transcript verbatim or list every utterance.\n\
- Do NOT invent facts that are not in the transcript.\n\
- Output plain text only — no preamble, no markdown fences, no title line.";

#[derive(Serialize, Clone)]
struct LiveSummaryEvent {
    summary: String,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn select_kk_key() -> Option<String> {
    keychain_store::get_select_kk_key()
}

fn trim_transcript(raw: &str) -> String {
    let t = raw.trim();
    if t.len() <= MAX_CHARS {
        return t.to_string();
    }
    // Keep the most recent context — older opener is less useful for a rolling summary.
    // Byte offset must land on a UTF-8 char boundary (Japanese is rarely aligned to MAX_CHARS).
    // Walk forward so the kept suffix stays ≤ MAX_CHARS (MSRV 1.80: no floor_char_boundary).
    let mut start = t.len().saturating_sub(MAX_CHARS);
    while start < t.len() && !t.is_char_boundary(start) {
        start += 1;
    }
    let slice = &t[start..];
    format!("…\n{slice}")
}

/// Kick a Live Summary refresh. Returns immediately; result arrives on `meeting_live_summary`.
#[tauri::command]
pub fn meeting_request_live_summary(app: tauri::AppHandle, transcript: String) -> Result<(), String> {
    let trimmed = trim_transcript(&transcript);
    let chars = trimmed.chars().count();
    if chars < MIN_CHARS {
        return Err(format!("need_more_context:{chars}"));
    }

    let now = now_ms();
    let last = LAST_STARTED_MS.load(Ordering::Acquire);
    if now - last < MIN_INTERVAL_MS {
        return Err("rate_limited".into());
    }
    if IN_FLIGHT
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("in_flight".into());
    }
    LAST_STARTED_MS.store(now, Ordering::Release);

    let Some(key) = select_kk_key() else {
        IN_FLIGHT.store(false, Ordering::Release);
        let _ = app.emit("meeting_live_summary_needs_key", ());
        return Err("needs_key".into());
    };

    let Some(db) = app.try_state::<Db>().map(|s| s.inner().clone()) else {
        IN_FLIGHT.store(false, Ordering::Release);
        return Err("no_db".into());
    };

    let app2 = app.clone();
    std::thread::spawn(move || {
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                IN_FLIGHT.store(false, Ordering::Release);
            }
        }
        let _guard = Guard;

        // Every exit from this worker must emit exactly one terminal event: the command already
        // returned Ok, so the FE has latched its in-flight guard and only an event clears it.
        // Returning silently here blocks Live Summary for the rest of the meeting.
        let (Ok(transport), Ok(rt)) = (
            ReqwestTransport::new(),
            tokio::runtime::Builder::new_current_thread().enable_all().build(),
        ) else {
            eprintln!("[meeting] live summary skipped — transport/runtime unavailable");
            let _ = app2.emit("meeting_live_summary_failed", ());
            return;
        };
        let client = AnthropicSelectKkMessagesClient::new(
            transport,
            db.traceability_sink(),
            SelectKkKey::new(Secret::new(key)),
            AnthropicConfig::new(SUMMARY_MODEL)
                .with_max_tokens(512)
                .with_temperature(0.2),
            SUMMARY_PURPOSE,
        );
        let user = shogun_core::llm::fence_untrusted(
            "Meeting transcript so far (speakers labeled when known). Summarize the meeting up to this point.",
            &trimmed,
        );
        match rt.block_on(client.complete_with_system(Some(SYSTEM), &user)) {
            Ok(text) => {
                let summary = text.trim().to_string();
                if summary.is_empty() {
                    eprintln!("[meeting] live summary returned empty");
                    let _ = app2.emit("meeting_live_summary_failed", ());
                    return;
                }
                let _ = app2.emit("meeting_live_summary", LiveSummaryEvent { summary });
            }
            Err(e) => {
                // Never log transcript content.
                eprintln!("[meeting] live summary failed: {e}");
                let _ = app2.emit("meeting_live_summary_failed", ());
            }
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_transcript_cuts_on_utf8_boundary() {
        // Each hiragana is 3 bytes — a naive byte slice at MAX_CHARS is mid-char.
        let unit = "あ";
        assert_eq!(unit.len(), 3);
        let need = MAX_CHARS / unit.len() + 8;
        let raw = unit.repeat(need);
        assert!(raw.len() > MAX_CHARS);

        let out = trim_transcript(&raw);
        assert!(out.starts_with("…\n"));
        let body = &out["…\n".len()..];
        assert!(body.len() <= MAX_CHARS);
        assert!(body.len() < raw.len());
        assert!(std::str::from_utf8(body.as_bytes()).is_ok());
        assert!(body.chars().all(|c| c == 'あ'));
    }

    #[test]
    fn trim_transcript_short_passthrough() {
        let raw = "hello meeting";
        assert_eq!(trim_transcript(raw), raw);
    }
}
