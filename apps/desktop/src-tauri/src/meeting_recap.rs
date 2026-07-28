//! MT4: the meeting Recap generator (FR-MT-19). Turns a closed interval's transcript + notes into
//! minutes (summary / decisions / next actions) over the Batch/Select-KK lane, then stores them.
//!
//! This is the desktop side of the same shape `dream.rs` uses: read the Select KK key from the
//! Keychain (account `select-kk-batch`), build a `ReqwestTransport` + a tokio runtime, and run one
//! Batch item through `AnthropicBatchClient`. Everything network here takes minutes, so it runs on
//! a spawned background thread — the meeting state machine never blocks on it.
//!
//! **Degradation is the rule, not the exception.** The meeting already has a degraded MT2 Recap
//! (`Db::meeting_recap`) the moment the interval closes. Every failure path here — no transcript,
//! no Select KK key, transport/runtime down, the batch never ending, an unparseable result, a DB
//! write failure — simply returns and leaves that degraded Recap in place (invariant / FR-MT-19).
//! Nothing here panics or crashes the app, and no transcript text is ever logged (code rule).
//!
//! Invariant 2/3: the summary chunk that leaves the device is recorded to traceability by the
//! Batch client's `submit` (one row per item, digest-only) tagged with the Recap purpose — this
//! module adds no second sink; it passes `db.traceability_sink()`.

#[cfg(target_os = "macos")]
pub use mac::spawn;

#[cfg(target_os = "macos")]
mod mac {
    use std::time::Duration;

    use shogun_core::daemon::Db;
    use shogun_core::meeting::minutes::{self, TranscriptLine};
    use shogun_core::meeting::settings::MeetingLanguage;
    use tauri::{Emitter, Manager};

    /// The Recap's Batch model. Small and fast on purpose: a per-meeting summarisation job that
    /// Select KK pays for. Mirrors the Dream Cycle's `BATCH_MODEL` choice (dream.rs).
    ///
    /// Interim, like dream.rs: once the batch relay lands the device stops naming a model and sends
    /// an intent instead (docs/batch-relay-design.md §4.4).
    const RECAP_MODEL: &str = "claude-haiku-4-5-20251001";

    /// Keychain coordinates of the Batch lane's credential — the *same* slot the Dream Cycle reads
    /// (dream.rs `KEYCHAIN_SERVICE` / `SELECT_KK_ACCOUNT`). One Select KK source, not a second.
    const KEYCHAIN_SERVICE: &str = "SHOGUN";
    const SELECT_KK_ACCOUNT: &str = "select-kk-batch";

    /// The traceability `purpose` tag carried on the summary chunk (read back as
    /// `traceview::Purpose::MeetingRecap`).
    const RECAP_PURPOSE: &str = "meeting_recap";

    /// Poll cadence and budget for the Recap batch. 30s × 20 ≈ 10 minutes — a meeting Recap should
    /// land soon after the meeting, so a shorter budget than the Dream Cycle's (60s × 120 ≈ 2h): a
    /// batch that stalls past this just leaves the degraded Recap in place (FR-MT-19), it never
    /// pins a thread for the API's 24-hour ceiling.
    const POLL_INTERVAL: Duration = Duration::from_secs(30);
    const MAX_POLLS: u32 = 20;

    /// The Select KK key, if this build has been provisioned with one. Absent is a normal state: the
    /// Recap then stays degraded rather than being generated. Read exactly like dream.rs.
    fn select_kk_key() -> Option<String> {
        security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, SELECT_KK_ACCOUNT)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Generate the Recap for the just-closed `session_id` on a background thread, then emit
    /// `meeting_recap` so the panel can refetch. Returns immediately; the meeting machine never
    /// blocks on the Batch. Every failure degrades silently to the existing MT2 Recap.
    pub fn spawn(app: &tauri::AppHandle, session_id: i64, language: MeetingLanguage) {
        // The DB is Arc-backed, so the clone shares the same connection (mirrors audio_lane).
        let Some(db) = app.try_state::<Db>().map(|s| s.inner().clone()) else {
            eprintln!("[meeting] no database for the Recap lane; keeping degraded recap");
            return;
        };
        let app = app.clone();
        std::thread::spawn(move || run(&app, &db, session_id, language));
    }

    /// The body of the background thread. Kept `&`-borrowing so the spawn closure owns the clones.
    fn run(app: &tauri::AppHandle, db: &Db, session_id: i64, language: MeetingLanguage) {
        // Gather the interval's transcript and note. Both may be empty — audio degrades to
        // notes-only, and a meeting may have had no note typed.
        let transcript = db.transcript_for_recap(session_id);
        let notes = db.meeting_note(session_id);

        let has_note = notes.as_deref().map(str::is_empty) == Some(false);
        if transcript.is_empty() && !has_note {
            // Nothing to summarise. The degraded Recap already says what little can be said.
            eprintln!("[meeting] nothing to summarise for session {session_id}; keeping degraded recap");
            return;
        }

        let lines: Vec<TranscriptLine> = transcript
            .iter()
            .map(|(speaker, text)| TranscriptLine { speaker: speaker.as_deref(), text })
            .collect();
        let prompt = minutes::build_prompt(&lines, notes.as_deref(), language.whisper_code().unwrap_or("en"));

        // The Select KK key. Absent → keep the degraded Recap (invariant 5 / FR-MT-19).
        let Some(key) = select_kk_key() else {
            eprintln!("[meeting] no Select KK key; keeping degraded recap");
            return;
        };

        // `generate` returns `None` on any degrade path, having already logged the specific reason.
        if let Some(mins) = generate(db, key, session_id, prompt) {
            let (dj, nj) = mins.to_columns();
            db.save_meeting_recap(session_id, &mins.summary, &dj, &nj, RECAP_MODEL);
            // Tell the panel to refetch — the degraded Recap is now the model's minutes.
            let _ = app.emit("meeting_recap", session_id);
            eprintln!("[meeting] recap generated for session {session_id}");
        }
    }

    /// Run the summary chunk through the Batch/Select-KK lane and parse the minutes. `None` on any
    /// transport/runtime/batch/parse failure — each logs one `[meeting] … ; keeping degraded recap`
    /// line (never any transcript content). Mirrors dream.rs's `run_via_batch` construction.
    fn generate(
        db: &Db,
        key: String,
        session_id: i64,
        prompt: String,
    ) -> Option<shogun_core::meeting::minutes::MeetingMinutes> {
        use shogun_core::llm::anthropic::{AnthropicBatchClient, AnthropicConfig, BatchItem};
        use shogun_core::llm::transport::ReqwestTransport;
        use shogun_core::llm::{Secret, SelectKkKey};

        let (Ok(transport), Ok(rt)) = (
            ReqwestTransport::new(),
            tokio::runtime::Builder::new_current_thread().enable_all().build(),
        ) else {
            eprintln!("[meeting] batch transport/runtime unavailable; keeping degraded recap");
            return None;
        };

        let client = AnthropicBatchClient::new(
            transport,
            db.traceability_sink(),
            SelectKkKey::new(Secret::new(key)),
            AnthropicConfig::new(RECAP_MODEL),
        );

        // One item: the whole meeting summarised at once. `custom_id` is the session id so the
        // single result is trivially keyed; `purpose` is the traceability tag (Purpose::MeetingRecap).
        let items = vec![BatchItem {
            custom_id: session_id.to_string(),
            purpose: RECAP_PURPOSE.to_string(),
            chunk: prompt,
        }];

        let results = match rt.block_on(client.run(&items, MAX_POLLS, || async {
            tokio::time::sleep(POLL_INTERVAL).await
        })) {
            Ok(r) => r,
            Err(e) => {
                // Provider / credential / never-ended: all degrade the same way here. The error is
                // the LlmError kind, never any prompt content.
                eprintln!("[meeting] recap batch failed ({e}); keeping degraded recap");
                return None;
            }
        };

        let Some(text) = results.into_iter().find_map(|r| r.text) else {
            eprintln!("[meeting] recap batch returned no text; keeping degraded recap");
            return None;
        };

        match minutes::parse_minutes(&text) {
            Ok(mins) => Some(mins),
            Err(_) => {
                // The parse error can carry a fragment of the model output; keep it out of the log.
                eprintln!("[meeting] recap summary unparseable; keeping degraded recap");
                None
            }
        }
    }
}
