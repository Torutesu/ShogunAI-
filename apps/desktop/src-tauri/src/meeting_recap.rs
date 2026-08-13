//! MT4: the meeting Recap generator (FR-MT-19). Turns a closed interval's transcript + notes into
//! minutes (summary / decisions / next actions) over the Batch/Select-KK lane, then stores them.
//!
//! This is the desktop side of the same shape `dream.rs` uses: read the Batch lane's credential
//! from the Keychain (account `select-kk-batch` — the license token on the shipping relay route,
//! a raw key only on the debug-gated direct route), build a `ReqwestTransport` + a tokio runtime,
//! and run one Batch item through the routed batch client (`shogun_core::llm::batch_route`).
//! Everything network here takes minutes, so it runs on a spawned background thread — the meeting
//! state machine never blocks on it.
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
pub use mac::{select_kk_configured, spawn};

#[cfg(target_os = "macos")]
mod mac {
    use std::time::Duration;

    use shogun_core::daemon::Db;
    use shogun_core::meeting::minutes::{self, TranscriptLine};
    use shogun_core::meeting::settings::MeetingLanguage;
    use shogun_integrations::keychain_store;
    use tauri::{Emitter, Manager};

    /// The direct lane's Recap model — **debug builds only**, mirroring dream.rs. On the
    /// shipping (relay) route the device sends a `model_class` intent and the relay chooses the
    /// model (docs/batch-relay-design.md §4.4).
    #[cfg(debug_assertions)]
    const RECAP_MODEL: &str = "claude-haiku-4-5-20251001";

    /// What the recap row records as its `model` on the relay route: the device truthfully knows
    /// only the intent it sent, not the model the relay chose.
    const RECAP_RELAY_MODEL_LABEL: &str = "select-relay/summarize";

    // Keychain coordinates of the Batch lane's credential — the *same* slot the Dream Cycle reads
    // (dream.rs / `keychain_store::SELECT_KK_ACCOUNT`). One Select KK source, not a second.

    /// The traceability `purpose` tag carried on the summary chunk (read back as
    /// `traceview::Purpose::MeetingRecap`).
    const RECAP_PURPOSE: &str = "meeting_recap";

    /// Poll cadence and budget for the Recap batch. 30s × 20 ≈ 10 minutes — a meeting Recap should
    /// land soon after the meeting, so a shorter budget than the Dream Cycle's (60s × 120 ≈ 2h): a
    /// batch that stalls past this just leaves the degraded Recap in place (FR-MT-19), it never
    /// pins a thread for the API's 24-hour ceiling.
    const POLL_INTERVAL: Duration = Duration::from_secs(30);
    const MAX_POLLS: u32 = 20;

    /// Whether the Batch lane's Select KK credential is present in Keychain. The overlay uses this
    /// to show a needs-key state only when Rust confirms absence — not on a UI timeout.
    pub fn select_kk_configured() -> bool {
        keychain_store::select_kk_configured()
    }

    /// The Select KK key, if this build has been provisioned with one. Absent is a normal state: the
    /// Recap then stays degraded rather than being generated. Read exactly like dream.rs.
    fn select_kk_key() -> Option<String> {
        keychain_store::get_select_kk_key()
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
            let _ = app.emit("meeting_recap_needs_key", session_id);
            return;
        };

        // `generate` returns `None` on any degrade path, having already logged the specific reason.
        if let Some((mins, model_label)) = generate(db, key, session_id, prompt) {
            let (dj, nj) = mins.to_columns();
            db.save_meeting_recap(session_id, &mins.summary, &dj, &nj, model_label);
            // Tell the panel to refetch — the degraded Recap is now the model's minutes.
            let _ = app.emit("meeting_recap", session_id);
            // The meeting just ended, so the mic is usually cold and this one can actually be
            // heard — the Ready case that most often reaches the user (#49).
            crate::sound::mac::play(shogun_core::sound::Cue::RecapReady);
            eprintln!("[meeting] recap generated for session {session_id}");
        }
    }

    /// Run the summary chunk through the Batch/Select-KK lane and parse the minutes, returning
    /// them with the model label the recap row should record. `None` on any
    /// transport/runtime/batch/parse failure — each logs one `[meeting] … ; keeping degraded
    /// recap` line (never any transcript content). Mirrors dream.rs's `run_via_batch` routing:
    /// the relay by default (license token; docs/batch-relay-design.md), direct Anthropic only in
    /// a debug build with the explicit env opt-in.
    fn generate(
        db: &Db,
        key: String,
        session_id: i64,
        prompt: String,
    ) -> Option<(shogun_core::meeting::minutes::MeetingMinutes, &'static str)> {
        use shogun_core::llm::anthropic::BatchItem;
        use shogun_core::llm::batch_route::{batch_route, BatchRoute, DEV_DIRECT_ENV};
        use shogun_core::llm::relay::{ModelClass, RelayBatchClient, RelayConfig};
        use shogun_core::llm::transport::ReqwestTransport;
        use shogun_core::llm::{Secret, SelectKkKey};

        let (Ok(transport), Ok(rt)) = (
            ReqwestTransport::new(),
            tokio::runtime::Builder::new_current_thread().enable_all().build(),
        ) else {
            eprintln!("[meeting] batch transport/runtime unavailable; keeping degraded recap");
            return None;
        };

        // One item: the whole meeting summarised at once. `custom_id` is the session id so the
        // single result is trivially keyed; `purpose` is the traceability tag (Purpose::MeetingRecap).
        let items = vec![BatchItem {
            custom_id: session_id.to_string(),
            purpose: RECAP_PURPOSE.to_string(),
            chunk: prompt,
        }];

        let credential = SelectKkKey::new(Secret::new(key));
        let (run_result, model_label) =
            match batch_route(std::env::var(DEV_DIRECT_ENV).ok().as_deref()) {
                BatchRoute::Relay => {
                    // Shipping path: the slot holds the license token; the relay picks the model.
                    let client = RelayBatchClient::new(
                        transport,
                        db.traceability_sink(),
                        credential,
                        RelayConfig::new(ModelClass::Summarize),
                    );
                    let r = rt.block_on(client.run(&items, MAX_POLLS, || async {
                        tokio::time::sleep(POLL_INTERVAL).await
                    }));
                    (r, RECAP_RELAY_MODEL_LABEL)
                }
                // Dev-only direct path (E-38): the slot holds a raw Anthropic key. The variant
                // does not exist in a release build, so this arm cannot ship.
                #[cfg(debug_assertions)]
                BatchRoute::DirectAnthropic => {
                    let client = shogun_core::llm::anthropic::AnthropicBatchClient::new(
                        transport,
                        db.traceability_sink(),
                        credential,
                        shogun_core::llm::anthropic::AnthropicConfig::new(RECAP_MODEL),
                    );
                    let r = rt.block_on(client.run(&items, MAX_POLLS, || async {
                        tokio::time::sleep(POLL_INTERVAL).await
                    }));
                    (r, RECAP_MODEL)
                }
            };

        let results = match run_result {
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
            Ok(mins) => Some((mins, model_label)),
            Err(_) => {
                // The parse error can carry a fragment of the model output; keep it out of the log.
                eprintln!("[meeting] recap summary unparseable; keeping degraded recap");
                None
            }
        }
    }
}
