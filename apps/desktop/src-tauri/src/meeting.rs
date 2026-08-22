//! Meeting notes: the adapter between the pure lifecycle (`shogun_core::meeting`) and macOS.
//!
//! The machine and the detection rules live in the core and are tested there. This file does the
//! three things that cannot be pure: it persists the settings, it drives a one-second tick so the
//! pill can show elapsed time and the offer can count down, and it projects the state into the
//! webview.
//!
//! Two invariants are kept structurally rather than by discipline:
//!
//! 1. **Off means the detector never runs.** [`on_focus`] returns before touching the machine when
//!    the feature is disabled, so nothing downstream can observe a meeting while it is off
//!    (FR-MT-02a).
//! 2. **Audio degrades, never crashes.** `Effect::StartAudio` opens the capture lane
//!    (`audio_lane`) against the interval the machine just opened; when audio is unavailable (no
//!    model, denied mic, no system tap) the lane returns nothing and the meeting still records the
//!    interval and the user's notes (FR-MT-13, MT3).

#[cfg(target_os = "macos")]
pub mod mac {
    mod overlay;
    mod persistence;
    mod state;

    use std::sync::{Arc, RwLock};

    use serde::Serialize;
    use shogun_core::meeting::settings::{MeetingLanguage, MeetingMode, Settings};
    use shogun_core::meeting::statemachine::{Input, State};

    pub use persistence::{init, on_focus, spawn_meeting_driver};
    pub use state::{is_recording, live_emit_allowed, MeetingView};

    use overlay::emit_settings;
    use persistence::save;
    use state::{
        apply, db, emit, finish_audio_stop, now_ms, set_live_emit_session, step, view, Lane, LANE,
    };

    /// MCP/REST/CLI run outside this process and persist only the meeting microphone. Refresh
    /// that field before a desktop mutation so a later UI save cannot erase an API selection.
    fn settings_with_persisted_microphone(settings: &Settings) -> Settings {
        let mut merged = settings.clone();
        merged.microphone = shogun_core::meeting::settings_store::load().microphone;
        merged
    }

    #[tauri::command]
    pub fn meeting_status() -> MeetingView {
        let now = now_ms();
        LANE.lock()
            .ok()
            .and_then(|g| g.as_ref().map(|l| view(l, now)))
            .unwrap_or(MeetingView {
                state: "idle",
                enabled: false,
                title: None,
                app_bundle_id: None,
                elapsed_ms: 0,
                countdown_ms: 0,
                paused: false,
                audio_error: None,
            })
    }

    #[tauri::command]
    pub fn meeting_wrapped(app: tauri::AppHandle) {
        overlay::meeting_wrapped(app);
    }

    #[tauri::command]
    pub fn meeting_drag(app: tauri::AppHandle, label: Option<String>) {
        overlay::meeting_drag(app, label);
    }

    #[tauri::command]
    pub fn meeting_overlay_dismiss(app: tauri::AppHandle) {
        overlay::meeting_overlay_dismiss(app);
    }

    #[tauri::command]
    pub fn meeting_set_overlay_panel(app: tauri::AppHandle, open: bool) {
        overlay::meeting_set_overlay_panel(app, open);
    }

    #[tauri::command]
    pub fn meeting_set_overlay_canvas(app: tauri::AppHandle, open: bool) {
        overlay::meeting_set_overlay_canvas(app, open);
    }

    #[tauri::command]
    pub fn meeting_set_overlay_chat(app: tauri::AppHandle, open: bool) {
        overlay::meeting_set_overlay_chat(app, open);
    }

    #[tauri::command]
    pub fn meeting_set_overlay_size(
        app: tauri::AppHandle,
        width: f64,
        height: f64,
        label: Option<String>,
    ) {
        overlay::meeting_set_overlay_size(app, width, height, label);
    }

    /// "Start" during the grace (FR-MT-08).
    #[tauri::command]
    pub fn meeting_start(app: tauri::AppHandle) {
        step(&app, Input::Start);
    }

    /// "Not now" — this meeting only; settings untouched (FR-MT-02c).
    #[tauri::command]
    pub fn meeting_not_now(app: tauri::AppHandle) {
        step(&app, Input::NotNow);
    }

    /// "Stop" — immediate, no confirmation dialog (FR-MT-09).
    #[tauri::command]
    pub fn meeting_stop(app: tauri::AppHandle) {
        let recording = LANE
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|l| l.machine.state() == State::Recording))
            .unwrap_or(false);
        if !recording {
            return;
        }
        step(&app, Input::Stop);
    }

    /// Toggle capture/ASR pause while keeping the meeting interval open.
    /// Pause stops feeding ASR (tears down the audio lane in RAM only — no waveform to disk).
    /// Resume restarts the lane against the same session. Stop still ends the meeting.
    ///
    /// Emit + unlock first; heavy audio start/stop runs after so the webview morph is not
    /// gated on device teardown / Deepgram reconnect.
    #[tauri::command]
    pub fn meeting_toggle_pause(app: tauri::AppHandle) {
        let now = now_ms();
        enum After {
            Stop(Option<crate::audio_lane::Handle>),
            Start {
                id: i64,
                live: Arc<RwLock<Settings>>,
            },
        }
        let after = {
            let Ok(mut g) = LANE.lock() else { return };
            let Some(lane) = g.as_mut() else { return };
            if lane.machine.state() != State::Recording {
                return;
            }
            if lane.paused {
                lane.paused = false;
                let start = lane.session_id.map(|id| {
                    set_live_emit_session(id);
                    if let Ok(mut live) = lane.live_settings.write() {
                        *live = lane.settings.clone();
                    }
                    After::Start {
                        id,
                        live: lane.live_settings.clone(),
                    }
                });
                eprintln!("[meeting] capture resumed (session held)");
                emit(&app, lane, now);
                start
            } else {
                lane.paused = true;
                set_live_emit_session(0);
                let handle = lane.audio.take();
                eprintln!("[meeting] capture paused (session held)");
                emit(&app, lane, now);
                Some(After::Stop(handle))
            }
        };
        match after {
            Some(After::Start { id, live }) => {
                // Resume: open devices off the command thread so invoke returns after emit.
                let app2 = app.clone();
                std::thread::spawn(move || {
                    let handle = crate::audio_lane::start(&app2, id, live);
                    if let Ok(mut g) = LANE.lock() {
                        if let Some(lane) = g.as_mut() {
                            // Only adopt the lane this worker actually started for. A fast
                            // pause/resume/pause/resume leaves two resume workers racing here, and
                            // Handle has no Drop-stop — overwriting a live `lane.audio` would leak
                            // a mic + Deepgram lane that keeps streaming and duplicating transcript
                            // lines. The session check keeps a worker whose meeting already ended
                            // from attaching to the *next* meeting's lane.
                            if lane.machine.state() == State::Recording
                                && !lane.paused
                                && lane.session_id == Some(id)
                                && lane.audio.is_none()
                            {
                                match handle {
                                    Ok(handle) => {
                                        lane.audio = Some(handle);
                                        return;
                                    }
                                    Err(error) => {
                                        set_live_emit_session(0);
                                        lane.audio_error = Some(error);
                                        emit(&app2, lane, now_ms());
                                        return;
                                    }
                                }
                            }
                        }
                    }
                    if let Ok(handle) = handle {
                        finish_audio_stop(Some(handle));
                    }
                });
            }
            Some(After::Stop(handle)) => {
                // Pause: join audio on a worker so the command returns immediately after emit.
                std::thread::spawn(move || finish_audio_stop(handle));
            }
            None => {}
        }
    }

    /// Save the note typed during the meeting (FR-MT-10). Silently does nothing when no interval
    /// is open — there is nothing to attach a note to, and losing the text is better than
    /// inventing a session for it.
    #[tauri::command]
    pub fn meeting_save_note(body: String, app: tauri::AppHandle) -> Result<(), String> {
        // Fall back to the just-finished interval: a note typed during the meeting is often
        // flushed (blur / debounce) moments after auto-wrap closed the session, and dropping it
        // then would lose exactly the text the user most wants kept.
        let id = LANE
            .lock()
            .ok()
            .and_then(|g| g.as_ref().and_then(|l| l.session_id.or(l.last_session_id)));
        let Some(id) = id else { return Ok(()) };
        let db = db(&app).ok_or("no database")?;
        // Report the failure. Swallowing it would tell the webview the note is safe while the
        // text the user typed is gone — the one piece of a meeting record that cannot be
        // regenerated (FR-MT-10).
        if db.save_meeting_note(id, &body) {
            Ok(())
        } else {
            Err("could not save the note".into())
        }
    }

    /// Whether the Select KK credential (`com.selectkk.shogun` / `select-kk-batch`) is in Keychain.
    /// overlay uses this so "needs key" is shown only when Rust confirms absence, not on a timeout.
    #[tauri::command]
    pub fn meeting_select_kk_configured() -> bool {
        crate::meeting_recap::select_kk_configured()
    }

    /// Deepgram API key presence for meeting live STT. Secrets are never returned in full.
    #[derive(Serialize)]
    pub struct DeepgramKeyStatus {
        pub has_key: bool,
        pub key_last4: String,
    }

    #[tauri::command]
    pub fn get_deepgram_key_status() -> DeepgramKeyStatus {
        match shogun_integrations::keychain_store::get_deepgram_asr_key() {
            Some(k) if !k.trim().is_empty() => {
                let k = k.trim();
                let n = k.chars().count();
                let last4 = if n >= 4 {
                    k.chars().skip(n - 4).collect()
                } else {
                    "····".to_string()
                };
                DeepgramKeyStatus {
                    has_key: true,
                    key_last4: last4,
                }
            }
            _ => DeepgramKeyStatus {
                has_key: false,
                key_last4: String::new(),
            },
        }
    }

    /// Save the Deepgram API key to Keychain (meeting live STT). The key value is never logged.
    #[tauri::command]
    pub fn set_deepgram_key(key: String) -> Result<(), String> {
        shogun_integrations::keychain_store::set_deepgram_asr_key(&key)?;
        eprintln!("[meeting] deepgram api key saved to Keychain");
        Ok(())
    }

    /// Remove the Deepgram API key from Keychain.
    #[tauri::command]
    pub fn clear_deepgram_key() -> Result<(), String> {
        match shogun_integrations::keychain_store::delete_generic_secret(
        shogun_integrations::keychain_store::DEEPGRAM_ASR_ACCOUNT,
    ) {
        Ok(()) => {}
        Err(e) if e.code() == -25300 /* errSecItemNotFound */ => {}
        Err(e) => return Err(e.to_string()),
    }
        eprintln!("[meeting] deepgram api key removed");
        Ok(())
    }

    /// The Recap for the most recently finished meeting (FR-MT-19), if there is one.
    ///
    /// Degraded by construction in MT2: assembled locally from the interval, the user's note and
    /// what was captured, with no model and no network.
    #[tauri::command]
    pub fn meeting_recap(app: tauri::AppHandle) -> Option<shogun_core::meeting::recap::Recap> {
        let id = LANE
            .lock()
            .ok()
            .and_then(|g| g.as_ref().and_then(|l| l.last_session_id))?;
        db(&app).and_then(|db| db.meeting_recap(id))
    }

    /// One suggested next action, as shown in the Recap card. `owner` is who the model thought
    /// should do it, when the transcript made that clear (never invented). L1/L3 discipline: this
    /// is a *suggestion* the panel displays, never something the app will do (invariant 4) — the
    /// card carries no "send"/"do it" affordance.
    #[derive(Serialize)]
    pub struct NextActionView {
        text: String,
        owner: Option<String>,
    }

    /// The model-generated minutes for the last finished meeting, shaped for the webview.
    ///
    /// The two structured columns are stored as JSON strings; we deserialize each here and, on a
    /// parse error, fall back to an empty list rather than failing the whole read (a malformed
    /// column must not blank the card — the degraded Recap is still shown underneath).
    #[derive(Serialize)]
    pub struct MinutesView {
        summary: String,
        decisions: Vec<String>,
        next_actions: Vec<NextActionView>,
    }

    /// The model-generated minutes for the most recently finished meeting (MT4, FR-MT-19), or
    /// `None` if the Batch lane has not produced them yet.
    ///
    /// This is layered on top of [`meeting_recap`], not a replacement: the degraded Recap shows the
    /// moment the interval closes, and these minutes arrive later (the panel refetches on the
    /// `meeting_recap` event). Reads the same `last_session_id` as [`meeting_recap`].
    #[tauri::command]
    pub fn meeting_recap_minutes(app: tauri::AppHandle) -> Option<MinutesView> {
        let id = LANE
            .lock()
            .ok()
            .and_then(|g| g.as_ref().and_then(|l| l.last_session_id))?;
        let stored = db(&app).and_then(|db| db.meeting_recap_full(id))?;
        let decisions: Vec<String> =
            serde_json::from_str(&stored.decisions_json).unwrap_or_default();
        let next_actions: Vec<shogun_core::meeting::minutes::NextAction> =
            serde_json::from_str(&stored.next_actions_json).unwrap_or_default();
        Some(MinutesView {
            summary: stored.summary,
            decisions,
            next_actions: next_actions
                .into_iter()
                .map(|a| NextActionView {
                    text: a.text,
                    owner: a.owner,
                })
                .collect(),
        })
    }

    /// One transcribed line for the post-meeting viewer (FR-MT-10). Shown only after Stop, never
    /// during recording.
    #[derive(Serialize)]
    pub struct TranscriptLineView {
        ts: i64,
        speaker: Option<String>,
        text: String,
    }

    /// Whisper marks silence as `[BLANK_AUDIO]`; the Recap viewer hides these but must not claim
    /// "no transcript" when they are the only rows stored.
    fn is_blank_transcript_marker(text: &str) -> bool {
        text.trim().eq_ignore_ascii_case("[BLANK_AUDIO]")
    }

    /// Displayable transcript lines plus a flag when only blank-audio markers were stored.
    #[derive(Serialize)]
    pub struct MeetingTranscriptView {
        lines: Vec<TranscriptLineView>,
        only_blanks: bool,
    }

    /// The session transcript for the most recently finished meeting, or for `session_id` when
    /// provided. Blank-audio markers are filtered out; `only_blanks` is set when that was all
    /// that was stored.
    #[tauri::command]
    pub fn get_meeting_transcript(
        app: tauri::AppHandle,
        session_id: Option<i64>,
    ) -> MeetingTranscriptView {
        let id = session_id.or_else(|| {
            LANE.lock()
                .ok()
                .and_then(|g| g.as_ref().and_then(|l| l.last_session_id))
        });
        let Some(id) = id else {
            return MeetingTranscriptView {
                lines: Vec::new(),
                only_blanks: false,
            };
        };
        let Some(db) = db(&app) else {
            return MeetingTranscriptView {
                lines: Vec::new(),
                only_blanks: false,
            };
        };
        let stored = db.meeting_transcript(id);
        let mut displayable = Vec::new();
        let mut saw_blank = false;
        for (ts, speaker, text) in stored {
            if text.trim().is_empty() {
                continue;
            }
            if is_blank_transcript_marker(&text) {
                saw_blank = true;
                continue;
            }
            displayable.push(TranscriptLineView { ts, speaker, text });
        }
        let only_blanks = displayable.is_empty() && saw_blank;
        eprintln!(
        "[meeting] get_meeting_transcript session={id}: {} displayable (only_blanks={only_blanks})",
        displayable.len()
    );
        MeetingTranscriptView {
            lines: displayable,
            only_blanks,
        }
    }

    /// Current settings for the Settings UI.
    #[tauri::command]
    pub fn get_meeting_settings() -> Settings {
        LANE.lock()
            .ok()
            .and_then(|mut g| {
                g.as_mut().map(|lane| {
                    lane.settings = settings_with_persisted_microphone(&lane.settings);
                    lane.settings.clone()
                })
            })
            .unwrap_or_default()
    }

    /// Tier (a): the whole feature on or off (FR-MT-02a).
    ///
    /// Switching off while a meeting is running ends it on the spot, through the same path as
    /// Stop — so the off switch reaches a meeting already in progress rather than only preventing
    /// the next one.
    ///
    /// **Persist first, then apply.** The other order lets a failed write leave the backend
    /// enabled while the settings screen (which rolls its toggle back on error) reads "Off" — the
    /// exact "off but something is running" state FR-MT-02a exists to forbid. It also means a
    /// user's "off" that failed to reach disk cannot come back as "on" after a restart.
    #[tauri::command]
    pub fn set_meeting_enabled(enabled: bool, app: tauri::AppHandle) -> Result<(), String> {
        let now = now_ms();
        let candidate = {
            let Ok(g) = LANE.lock() else {
                return Err("busy".into());
            };
            let Some(lane) = g.as_ref() else {
                return Err("not ready".into());
            };
            let mut candidate = settings_with_persisted_microphone(&lane.settings);
            candidate.enabled = enabled;
            candidate
        };
        save(&app, &candidate)?;

        let Ok(mut g) = LANE.lock() else {
            return Err("busy".into());
        };
        let Some(lane) = g.as_mut() else {
            return Err("not ready".into());
        };
        lane.settings = candidate.clone();
        if let Ok(mut live) = lane.live_settings.write() {
            *live = candidate;
        }
        if !enabled {
            let effects = lane.machine.step(Input::FeatureDisabled);
            let stop_audio = apply(&app, lane, &effects, now);
            drop(g);
            finish_audio_stop(stop_audio);
        } else {
            emit(&app, lane, now);
        }
        eprintln!(
            "[meeting] notes → {}",
            if enabled { "enabled" } else { "off" }
        );
        Ok(())
    }

    /// Mic-only detection opt-in (FR-MT-04). Ships off: sustained mic alone never offers unless
    /// the user enables this in settings.
    #[tauri::command]
    pub fn set_meeting_allow_mic_only(allow: bool, app: tauri::AppHandle) -> Result<(), String> {
        let candidate = {
            let Ok(g) = LANE.lock() else {
                return Err("busy".into());
            };
            let Some(lane) = g.as_ref() else {
                return Err("not ready".into());
            };
            let mut candidate = settings_with_persisted_microphone(&lane.settings);
            candidate.allow_mic_only_detect = allow;
            candidate
        };
        save(&app, &candidate)?;

        let Ok(mut g) = LANE.lock() else {
            return Err("busy".into());
        };
        let Some(lane) = g.as_mut() else {
            return Err("not ready".into());
        };
        lane.settings = candidate.clone();
        if let Ok(mut live) = lane.live_settings.write() {
            *live = candidate;
        }
        eprintln!(
            "[meeting] mic-only detect → {}",
            if allow { "on" } else { "off" }
        );
        Ok(())
    }

    /// Select the microphone used for the next meeting. `None` follows the macOS default.
    /// Capture refuses a missing selection instead of silently using another input.
    #[derive(serde::Serialize)]
    pub struct MeetingMicrophone {
        name: String,
        ambiguous: bool,
    }

    #[tauri::command(async)]
    pub fn get_meeting_microphones() -> Result<Vec<MeetingMicrophone>, String> {
        shogun_core::audio::capture::mic::input_device_choices().map(|choices| {
            choices
                .into_iter()
                .map(|choice| MeetingMicrophone {
                    name: choice.name,
                    ambiguous: choice.ambiguous,
                })
                .collect()
        })
    }

    #[tauri::command]
    pub fn set_meeting_microphone(
        microphone: Option<String>,
        app: tauri::AppHandle,
    ) -> Result<(), String> {
        let candidate = {
            let Ok(g) = LANE.lock() else {
                return Err("busy".into());
            };
            let Some(lane) = g.as_ref() else {
                return Err("not ready".into());
            };
            Settings {
                microphone: microphone.filter(|name| !name.trim().is_empty()),
                ..lane.settings.clone()
            }
        };
        save(&app, &candidate)?;

        let Ok(mut g) = LANE.lock() else {
            return Err("busy".into());
        };
        let Some(lane) = g.as_mut() else {
            return Err("not ready".into());
        };
        lane.settings = candidate.clone();
        if let Ok(mut live) = lane.live_settings.write() {
            *live = candidate;
        }
        Ok(())
    }

    /// If recording (not paused) and ASR language changed, stop the live lane so a restart can
    /// reopen Deepgram/whisper with the new hint. Without this, Transcribe→One-way mid-meeting
    /// keeps English-only ASR and Japanese never reaches the translator.
    fn take_asr_restart(
        lane: &mut Lane,
        prev_asr: MeetingLanguage,
    ) -> Option<(
        i64,
        Arc<RwLock<Settings>>,
        Option<crate::audio_lane::Handle>,
    )> {
        let new_asr = lane.settings.asr_language();
        if prev_asr == new_asr {
            return None;
        }
        if lane.machine.state() != State::Recording || lane.paused {
            return None;
        }
        let id = lane.session_id?;
        let handle = lane.audio.take();
        set_live_emit_session(id);
        Some((id, lane.live_settings.clone(), handle))
    }

    fn run_asr_restart(
        app: tauri::AppHandle,
        id: i64,
        live: Arc<RwLock<Settings>>,
        old: Option<crate::audio_lane::Handle>,
    ) {
        std::thread::spawn(move || {
            finish_audio_stop(old);
            let handle = crate::audio_lane::start(&app, id, live);
            if let Ok(mut g) = LANE.lock() {
                if let Some(lane) = g.as_mut() {
                    if lane.machine.state() == State::Recording
                        && !lane.paused
                        && lane.session_id == Some(id)
                        && lane.audio.is_none()
                    {
                        match handle {
                            Ok(handle) => {
                                lane.audio = Some(handle);
                                eprintln!("[meeting] ASR lane restarted (language/mode change)");
                                return;
                            }
                            Err(error) => {
                                set_live_emit_session(0);
                                lane.audio_error = Some(error);
                                emit(&app, lane, now_ms());
                                return;
                            }
                        }
                    }
                }
            }
            if let Ok(handle) = handle {
                finish_audio_stop(Some(handle));
            }
        });
    }

    /// In-meeting overlay mode (issue #93). Applies to new lines when changed mid-recording.
    /// Restarts the audio lane when `asr_language()` changes (e.g. Transcribe → One-way).
    #[tauri::command]
    pub fn set_meeting_mode(mode: MeetingMode, app: tauri::AppHandle) -> Result<(), String> {
        let now = now_ms();
        let (candidate, prev_asr) = {
            let Ok(g) = LANE.lock() else {
                return Err("busy".into());
            };
            let Some(lane) = g.as_ref() else {
                return Err("not ready".into());
            };
            let mut candidate = settings_with_persisted_microphone(&lane.settings);
            candidate.meeting_mode = mode;
            (candidate, lane.settings.asr_language())
        };
        save(&app, &candidate)?;

        let restart = {
            let Ok(mut g) = LANE.lock() else {
                return Err("busy".into());
            };
            let Some(lane) = g.as_mut() else {
                return Err("not ready".into());
            };
            lane.settings = candidate.clone();
            if let Ok(mut live) = lane.live_settings.write() {
                *live = candidate;
            }
            let restart = take_asr_restart(lane, prev_asr);
            emit(&app, lane, now);
            emit_settings(&app, &lane.settings);
            restart
        };
        if let Some((id, live, old)) = restart {
            run_asr_restart(app, id, live, old);
        }
        Ok(())
    }

    /// Language pair for one-way / two-way translation modes.
    /// Restarts ASR when the resolved `asr_language()` changes (e.g. Auto → Japanese).
    ///
    /// `rename_all = "snake_case"`: the overlay sends the settings field names verbatim
    /// (`source_lang`, …). Tauri v2's default argument case is camelCase with NO snake_case
    /// fallback, so without this every field deserialized to None and the picker wrote the
    /// existing settings back — the language selection visibly snapped back and translation
    /// kept running in the old language.
    #[tauri::command(rename_all = "snake_case")]
    pub fn set_meeting_langs(
        source_lang: Option<MeetingLanguage>,
        target_lang: Option<MeetingLanguage>,
        my_lang: Option<MeetingLanguage>,
        other_lang: Option<MeetingLanguage>,
        app: tauri::AppHandle,
    ) -> Result<(), String> {
        let now = now_ms();
        let (candidate, prev_asr) = {
            let Ok(g) = LANE.lock() else {
                return Err("busy".into());
            };
            let Some(lane) = g.as_ref() else {
                return Err("not ready".into());
            };
            let mut candidate = settings_with_persisted_microphone(&lane.settings);
            candidate.source_lang = source_lang.unwrap_or(lane.settings.source_lang);
            candidate.target_lang = target_lang.unwrap_or(lane.settings.target_lang);
            candidate.my_lang = my_lang.unwrap_or(lane.settings.my_lang);
            candidate.other_lang = other_lang.unwrap_or(lane.settings.other_lang);
            (candidate, lane.settings.asr_language())
        };
        save(&app, &candidate)?;

        let restart = {
            let Ok(mut g) = LANE.lock() else {
                return Err("busy".into());
            };
            let Some(lane) = g.as_mut() else {
                return Err("not ready".into());
            };
            lane.settings = candidate.clone();
            if let Ok(mut live) = lane.live_settings.write() {
                *live = candidate;
            }
            let restart = take_asr_restart(lane, prev_asr);
            emit(&app, lane, now);
            emit_settings(&app, &lane.settings);
            restart
        };
        if let Some((id, live, old)) = restart {
            run_asr_restart(app, id, live, old);
        }
        Ok(())
    }

    #[tauri::command]
    pub fn meeting_include_app(bundle_id: String, app: tauri::AppHandle) -> Result<(), String> {
        let settings = {
            let Ok(mut g) = LANE.lock() else {
                return Err("busy".into());
            };
            let Some(lane) = g.as_mut() else {
                return Err("not ready".into());
            };
            let mut candidate = settings_with_persisted_microphone(&lane.settings);
            candidate.excluded_apps.remove(&bundle_id);
            lane.settings = candidate.clone();
            candidate
        };
        save(&app, &settings)
    }

    /// Tier (b): never offer for this app again (FR-MT-02b).
    #[tauri::command]
    pub fn meeting_exclude_app(bundle_id: String, app: tauri::AppHandle) -> Result<(), String> {
        let now = now_ms();
        let candidate = {
            let Ok(g) = LANE.lock() else {
                return Err("busy".into());
            };
            let Some(lane) = g.as_ref() else {
                return Err("not ready".into());
            };
            let mut next = settings_with_persisted_microphone(&lane.settings);
            next.exclude_app(&bundle_id);
            next
        };
        save(&app, &candidate)?;

        let Ok(mut g) = LANE.lock() else {
            return Err("busy".into());
        };
        let Some(lane) = g.as_mut() else {
            return Err("not ready".into());
        };
        lane.settings = candidate;
        // Excluding from the offer panel also declines whatever prompted it — from Offered that
        // is the pending offer, and from Recording the meeting in progress.
        let input = if lane.machine.state() == State::Recording {
            Input::Stop
        } else {
            Input::NotNow
        };
        let effects = lane.machine.step(input);
        let stop_audio = apply(&app, lane, &effects, now);
        drop(g);
        finish_audio_stop(stop_audio);
        Ok(())
    }
}
