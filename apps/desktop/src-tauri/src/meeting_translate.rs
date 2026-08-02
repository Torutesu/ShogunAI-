//! Live EN→JA translation for the meeting overlay (issue #93). JA→EN uses on-device whisper
//! translate in the audio worker; this module handles the reverse direction asynchronously so ASR
//! lines appear immediately and translation fills in when ready (Select KK Messages API — sync,
//! typically 1–3s per short line; Batch is reserved for recap/dream).

#[cfg(target_os = "macos")]
pub use mac::{should_translate_asr, spawn_ja_translation};

#[cfg(target_os = "macos")]
mod mac {
    use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;

    use serde::Serialize;
    use shogun_core::daemon::Db;
    use shogun_core::llm::anthropic::{AnthropicConfig, AnthropicSelectKkMessagesClient};
    use shogun_core::llm::transport::ReqwestTransport;
    use shogun_core::llm::{LlmError, Secret, SelectKkKey};
    use tauri::Emitter;

    const KEYCHAIN_SERVICE: &str = "SHOGUN";
    const SELECT_KK_ACCOUNT: &str = "select-kk-batch";
    const TRANSLATE_MODEL: &str = "claude-haiku-4-5-20251001";
    const TRANSLATE_PURPOSE: &str = "meeting_live_translate";
    const TRANSLATE_SYSTEM: &str =
        "Translate to Japanese. Output ONLY the translation. No preamble.";
    /// Cap concurrent network translates so a fast talker cannot stack threads.
    const MAX_IN_FLIGHT: usize = 3;
    /// After a 429, pause new translate requests briefly.
    const RATE_LIMIT_COOLDOWN_MS: i64 = 30_000;
    /// Skip duplicate ASR within this window (ms).
    const DEDUP_WINDOW_MS: i64 = 2_500;
    const MAX_RATE_LIMIT_RETRIES: u32 = 4;
    const RATE_LIMIT_BACKOFF_MS: [u64; 4] = [0, 1_000, 2_000, 4_000];

    static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
    static RATE_LIMIT_UNTIL_MS: AtomicI64 = AtomicI64::new(0);
    static LAST_SOURCE: Mutex<Option<(String, i64)>> = Mutex::new(None);

    #[derive(Serialize, Clone)]
    struct TranslationEvent {
        ts: i64,
        translation: String,
    }

    fn now_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    fn rate_limit_active() -> bool {
        RATE_LIMIT_UNTIL_MS.load(Ordering::Acquire) > now_ms()
    }

    fn arm_rate_limit_cooldown() {
        RATE_LIMIT_UNTIL_MS.store(now_ms() + RATE_LIMIT_COOLDOWN_MS, Ordering::Release);
    }

    fn select_kk_key() -> Option<String> {
        security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, SELECT_KK_ACCOUNT)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    fn log_translate_err(ts: i64, err: &LlmError) {
        match err {
            LlmError::Unauthorized(status, _) => {
                eprintln!("[meeting] live translate ts={ts} failed: HTTP {status} (check SHOGUN/select-kk-batch Keychain entry)");
            }
            LlmError::RateLimited(status, _) => {
                eprintln!("[meeting] live translate ts={ts} failed: HTTP {status} (rate limited, keeping ASR line)");
            }
            other => eprintln!("[meeting] live translate ts={ts} failed: {other}"),
        }
    }

    /// Skip blank, music-only, or non-speech ASR before hitting the network.
    pub fn should_translate_asr(text: &str) -> bool {
        let raw = text.trim();
        if raw.is_empty() {
            return false;
        }

        let without_music: String = raw
            .chars()
            .filter(|c| !matches!(c, '♪' | '♫' | '♬' | '🎵' | '🎶'))
            .collect();
        let stripped = without_music.trim();
        if stripped.is_empty() {
            return false;
        }

        let lower = stripped.to_lowercase();
        if lower.starts_with('[') && lower.ends_with(']') {
            let inner = lower[1..lower.len().saturating_sub(1)].trim();
            if matches!(
                inner,
                "music"
                    | "applause"
                    | "silence"
                    | "laughter"
                    | "inaudible"
                    | "background music"
                    | "instrumental"
            ) {
                return false;
            }
        }

        if stripped
            .chars()
            .all(|c| c.is_ascii_punctuation() || c.is_whitespace())
        {
            return false;
        }

        // Whisper sometimes emits bare filler tokens on noise.
        if stripped.len() <= 2 && !stripped.chars().any(|c| c.is_alphabetic()) {
            return false;
        }

        true
    }

    fn is_duplicate_source(source: &str, ts: i64) -> bool {
        let Ok(guard) = LAST_SOURCE.lock() else {
            return false;
        };
        guard
            .as_ref()
            .is_some_and(|(last_text, last_ts)| last_text == source && (ts - last_ts).abs() < DEDUP_WINDOW_MS)
    }

    fn remember_source(source: &str, ts: i64) {
        if let Ok(mut guard) = LAST_SOURCE.lock() {
            *guard = Some((source.to_string(), ts));
        }
    }

    /// Strip wrapping quotes / markdown fences models sometimes add despite instructions.
    fn sanitize_translation(raw: &str) -> String {
        let mut text = raw.trim().to_string();
        if text.len() >= 2 {
            let bytes = text.as_bytes();
            let quote = bytes[0];
            if (quote == b'"' || quote == b'\'') && bytes[text.len() - 1] == quote {
                text = text[1..text.len() - 1].trim().to_string();
            }
        }
        if text.starts_with("```") {
            if let Some(rest) = text.strip_prefix("```") {
                let rest = rest.trim_start_matches(|c: char| c.is_alphanumeric() || c == '-');
                if let Some(inner) = rest.strip_suffix("```") {
                    text = inner.trim().to_string();
                }
            }
        }
        text
    }

    /// Detect assistant meta-chat / refusal instead of a translation.
    pub fn looks_like_refusal(text: &str) -> bool {
        let lower = text.to_lowercase();
        const NEEDLES: &[&str] = &[
            "i don't see",
            "i do not see",
            "could you please",
            "please provide",
            "provide the",
            "provide me",
            "spoken line",
            "audio content",
            "no text to translate",
            "no audio",
            "i'm sorry",
            "i am sorry",
            "as an ai",
            "i cannot translate",
            "i can't translate",
            "unable to translate",
            "need the text",
            "share the text",
            "you'd like translated",
            "would you like",
            "can you provide",
            "don't have any text",
            "do not have any text",
            "there is no text",
            "there's no text",
            "nothing to translate",
        ];
        if NEEDLES.iter().any(|needle| lower.contains(needle)) {
            return true;
        }
        const FLUFF: &[&str] = &[
            "sure,",
            "sure!",
            "certainly",
            "of course",
            "here is the translation",
            "here's the translation",
            "the translation is",
        ];
        FLUFF.iter().any(|prefix| lower.starts_with(prefix))
    }

    async fn translate_with_backoff<T: shogun_core::llm::transport::HttpTransport, S: shogun_core::llm::traceability::TraceabilitySink>(
        client: &AnthropicSelectKkMessagesClient<T, S>,
        source: &str,
        ts: i64,
    ) -> Result<String, LlmError> {
        let mut last_err = LlmError::RateLimited(429, "exhausted retries".into());
        for attempt in 0..MAX_RATE_LIMIT_RETRIES {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(
                    RATE_LIMIT_BACKOFF_MS[attempt as usize],
                ))
                .await;
            }
            match client
                .complete_with_system(Some(TRANSLATE_SYSTEM), source)
                .await
            {
                Ok(t) => return Ok(t),
                Err(e @ LlmError::RateLimited(status, _)) => {
                    eprintln!(
                        "[meeting] live translate ts={ts} rate limited (HTTP {status}), retry {}/{}",
                        attempt + 1,
                        MAX_RATE_LIMIT_RETRIES
                    );
                    arm_rate_limit_cooldown();
                    last_err = e;
                }
                Err(e) => return Err(e),
            }
        }
        Err(last_err)
    }

    /// Translate one line to Japanese on a background thread and emit `meeting_live_translation`.
    pub fn spawn_ja_translation(
        app: &tauri::AppHandle,
        db: Db,
        session_id: i64,
        ts: i64,
        text: String,
    ) {
        let source = text.trim().to_string();
        if !should_translate_asr(&source) {
            eprintln!("[meeting] live translate ts={ts} skipped — non-speech/blank ASR");
            return;
        }
        if rate_limit_active() {
            eprintln!("[meeting] live translate ts={ts} skipped — rate-limit cooldown active");
            return;
        }
        if is_duplicate_source(&source, ts) {
            eprintln!("[meeting] live translate ts={ts} skipped — duplicate ASR within {DEDUP_WINDOW_MS}ms");
            return;
        }

        if IN_FLIGHT.fetch_add(1, Ordering::AcqRel) >= MAX_IN_FLIGHT {
            IN_FLIGHT.fetch_sub(1, Ordering::AcqRel);
            eprintln!("[meeting] live translate ts={ts} skipped — {MAX_IN_FLIGHT} already in flight");
            return;
        }

        remember_source(&source, ts);
        let app = app.clone();
        std::thread::spawn(move || {
            struct Guard;
            impl Drop for Guard {
                fn drop(&mut self) {
                    IN_FLIGHT.fetch_sub(1, Ordering::AcqRel);
                }
            }
            let _guard = Guard;

            let Some(key) = select_kk_key() else {
                eprintln!("[meeting] live translate ts={ts} skipped — no Select KK key (SHOGUN/select-kk-batch)");
                return;
            };
            let (Ok(transport), Ok(rt)) = (
                ReqwestTransport::new(),
                tokio::runtime::Builder::new_current_thread().enable_all().build(),
            ) else {
                eprintln!("[meeting] live translate ts={ts} skipped — transport/runtime unavailable");
                return;
            };
            let client = AnthropicSelectKkMessagesClient::new(
                transport,
                db.traceability_sink(),
                SelectKkKey::new(Secret::new(key)),
                AnthropicConfig::new(TRANSLATE_MODEL)
                    .with_max_tokens(256)
                    .with_temperature(0.0),
                TRANSLATE_PURPOSE,
            );
            let translated = match rt.block_on(translate_with_backoff(&client, &source, ts)) {
                Ok(t) => sanitize_translation(&t),
                Err(e) => {
                    log_translate_err(ts, &e);
                    return;
                }
            };
            if translated.is_empty() {
                eprintln!("[meeting] live translate ts={ts} returned empty (keeping ASR line)");
                return;
            }
            if looks_like_refusal(&translated) {
                eprintln!(
                    "[meeting] live translate ts={ts} dropped refusal/meta-chat (keeping ASR line on overlay)"
                );
                return;
            }
            if !crate::meeting::mac::live_emit_allowed(session_id) {
                return;
            }
            let _ = app.emit(
                "meeting_live_translation",
                TranslationEvent { ts, translation: translated },
            );
        });
    }

    #[cfg(test)]
    mod tests {
        use super::{looks_like_refusal, sanitize_translation, should_translate_asr};

        #[test]
        fn refusal_detects_meta_chat() {
            assert!(looks_like_refusal(
                "I don't see any audio content or text to translate. Could you please provide the spoken line?"
            ));
            assert!(!looks_like_refusal("会議の資料を共有してください。"));
        }

        #[test]
        fn sanitize_strips_wrapping_quotes_and_fences() {
            assert_eq!(sanitize_translation("\"こんにちは\""), "こんにちは");
            assert_eq!(sanitize_translation("```\n翻訳文\n```"), "翻訳文");
        }

        #[test]
        fn skip_music_only_asr() {
            assert!(!should_translate_asr("♪ ♪ ♪"));
            assert!(!should_translate_asr("[Music]"));
            assert!(!should_translate_asr("   "));
            assert!(should_translate_asr("Let's sync on the roadmap."));
        }
    }
}
