//! Sound cues: sense the environment, ask the policy, and only then make a noise (#49).
//!
//! The decision itself is not here — it is `shogun_core::sound::should_play`, so that the rules
//! that keep SHOGUN quiet (a live microphone above all) are testable without a Mac. This module
//! is the adapter: it reads the three environment facts the policy needs, holds the user's
//! settings, and plays a preloaded `NSSound` when the answer is yes.
//!
//! Why the WAVs are compiled in (`include_bytes!`) rather than bundled as resources: a cue that
//! silently fails to load is indistinguishable from the policy deciding to stay silent, and the
//! six files together are ~158 KB. Embedding removes the dev/bundle path split, the disk read at
//! startup, and that whole class of "why is it quiet?" bug.

#[cfg(target_os = "macos")]
pub mod mac {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::time::Instant;

    use objc2::rc::Retained;
    use objc2::AnyThread;
    use objc2_app_kit::NSSound;
    use objc2_foundation::NSData;
    use tauri::{AppHandle, Manager};

    use shogun_core::sound::{self, Category, Cue, Env, Pref, QuietHours, Settings, Verdict};

    /// The six cue files, compiled in. Regenerate with `scripts/generate-cue-sounds.py`.
    const CUE_WAVS: [(&str, &[u8]); 6] = [
        ("ack-open", include_bytes!("../sounds/ack-open.wav")),
        ("ack-close", include_bytes!("../sounds/ack-close.wav")),
        ("ready", include_bytes!("../sounds/ready.wav")),
        ("ask", include_bytes!("../sounds/ask.wav")),
        ("fail", include_bytes!("../sounds/fail.wav")),
        ("signature", include_bytes!("../sounds/signature.wav")),
    ];

    thread_local! {
        /// Decoded players, main thread only — `NSSound` is not `Send`, and AppKit objects in this
        /// app are only ever touched on the main thread. Preloading is what keeps playback off the
        /// I/O path: a cue can share a frame with the 100 ms expand SLO.
        static PLAYERS: RefCell<HashMap<&'static str, Retained<NSSound>>> =
            RefCell::new(HashMap::new());
    }

    struct State {
        settings: Settings,
        /// When each cue last actually played, for the repeat throttle.
        last_played: HashMap<&'static str, Instant>,
    }

    static STATE: Mutex<Option<State>> = Mutex::new(None);

    fn settings_path(app: &AppHandle) -> Option<std::path::PathBuf> {
        app.path().app_data_dir().ok().map(|d| d.join("sound.json"))
    }

    /// Load settings and preload the players. Any failure leaves the defaults in place —
    /// unreadable settings must never make the app louder than the user asked for.
    pub fn init(app: &AppHandle) {
        let settings = settings_path(app)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str::<Settings>(&t).ok())
            .unwrap_or_default();
        if let Ok(mut g) = STATE.lock() {
            *g = Some(State { settings, last_played: HashMap::new() });
        }
        let _ = app.run_on_main_thread(preload);
        eprintln!(
            "[sound] cues {} (quiet hours {})",
            settings.pref.tag(),
            if settings.quiet_hours.enabled { "on" } else { "off" }
        );
    }

    /// Build one `NSSound` per asset from the embedded bytes. Main thread only.
    fn preload() {
        PLAYERS.with(|players| {
            let Ok(mut players) = players.try_borrow_mut() else { return };
            if !players.is_empty() {
                return;
            }
            for (name, bytes) in CUE_WAVS {
                // SAFETY: `NSData::with_bytes` copies, so the slice does not need to outlive the
                // call, and `initWithData:` is the documented initialiser for in-memory audio.
                let sound = unsafe {
                    let data = NSData::with_bytes(bytes);
                    NSSound::initWithData(NSSound::alloc(), &data)
                };
                match sound {
                    Some(s) => {
                        players.insert(name, s);
                    }
                    // Not fatal: the product keeps working, silently. Say so once, loudly enough
                    // to be found in a log, because "no sound" otherwise looks like policy.
                    None => eprintln!("[sound] cue asset {name} failed to load; it will be silent"),
                }
            }
        });
    }

    pub fn settings() -> Settings {
        STATE.lock().ok().and_then(|g| g.as_ref().map(|s| s.settings)).unwrap_or_default()
    }

    fn save(app: &AppHandle, settings: &Settings) -> Result<(), String> {
        let Some(p) = settings_path(app) else { return Ok(()) };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
        std::fs::write(&p, json).map_err(|e| format!("save failed: {e}"))
    }

    fn update(app: &AppHandle, f: impl FnOnce(&mut Settings)) -> Result<Settings, String> {
        let next = {
            let mut g = STATE.lock().map_err(|_| "sound state unavailable".to_string())?;
            let state = g.get_or_insert_with(|| State {
                settings: Settings::default(),
                last_played: HashMap::new(),
            });
            f(&mut state.settings);
            state.settings
        };
        save(app, &next)?;
        Ok(next)
    }

    // ── environment ─────────────────────────────────────────────────────────────────────────

    /// Minutes since local midnight. Same `localtime_r` route the Dream Cycle uses, so DST and a
    /// mid-session zone change are already folded in.
    fn now_local_min() -> u16 {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        // SAFETY: `tm` is written by localtime_r before it is read, and `t` outlives the call.
        unsafe {
            let mut tm: libc::tm = std::mem::zeroed();
            let t = secs as libc::time_t;
            if libc::localtime_r(&t, &mut tm).is_null() {
                // Unknown local time: report midday, the one value that cannot accidentally land
                // inside a night-time quiet window and silence a failure cue.
                12 * 60
            } else {
                (tm.tm_hour as u16) * 60 + tm.tm_min as u16
            }
        }
    }

    /// System Settings → Sound → "Play user interface sound effects" (D5).
    ///
    /// The key lives in the global domain, which `standardUserDefaults` searches. Absent (the
    /// setting was never touched) means on, which is the macOS default.
    fn os_ui_sounds_enabled() -> bool {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};
        use objc2_foundation::NSString;

        const KEY: &str = "com.apple.sound.uiaudio.enabled";
        unsafe {
            let defaults: *mut AnyObject = msg_send![class!(NSUserDefaults), standardUserDefaults];
            if defaults.is_null() {
                return true;
            }
            let key = NSString::from_str(KEY);
            // `objectForKey:` first: `boolForKey:` cannot distinguish "off" from "never set".
            let obj: *mut AnyObject = msg_send![defaults, objectForKey: &*key];
            if obj.is_null() {
                return true;
            }
            msg_send![defaults, boolForKey: &*key]
        }
    }

    /// Whether the default output is the built-in speaker — i.e. whether anything we play can be
    /// heard by the built-in microphone and, through it, by everyone else on the call.
    ///
    /// Failure answers `true`: assuming the speaker keeps us quiet, and a wrong "quiet" costs a
    /// chime, while a wrong "safe to play" costs a chime *in someone else's meeting*
    /// (docs/sound-design.md §8.3).
    fn output_is_builtin_speaker() -> bool {
        use std::ffi::c_void;

        type AudioObjectID = u32;
        type OSStatus = i32;

        #[repr(C)]
        struct AudioObjectPropertyAddress {
            selector: u32,
            scope: u32,
            element: u32,
        }

        const fn fourcc(s: &[u8; 4]) -> u32 {
            ((s[0] as u32) << 24) | ((s[1] as u32) << 16) | ((s[2] as u32) << 8) | (s[3] as u32)
        }

        const SYSTEM_OBJECT: AudioObjectID = 1;
        const DEFAULT_OUTPUT_DEVICE: u32 = fourcc(b"dOut");
        const TRANSPORT_TYPE: u32 = fourcc(b"tran");
        const SCOPE_GLOBAL: u32 = fourcc(b"glob");
        const ELEMENT_MAIN: u32 = 0;
        const TRANSPORT_BUILTIN: u32 = fourcc(b"bltn");
        const NO_ERROR: OSStatus = 0;

        #[link(name = "CoreAudio", kind = "framework")]
        extern "C" {
            fn AudioObjectGetPropertyData(
                object: AudioObjectID,
                address: *const AudioObjectPropertyAddress,
                qualifier_size: u32,
                qualifier: *const c_void,
                data_size: *mut u32,
                data: *mut c_void,
            ) -> OSStatus;
        }

        /// One `u32` property, or `None` when CoreAudio will not answer.
        fn read_u32(object: AudioObjectID, selector: u32) -> Option<u32> {
            let addr = AudioObjectPropertyAddress {
                selector,
                scope: SCOPE_GLOBAL,
                element: ELEMENT_MAIN,
            };
            let mut value: u32 = 0;
            let mut size = std::mem::size_of::<u32>() as u32;
            // SAFETY: `addr` and `value` outlive the call; `size` matches `value`'s size, which is
            // what CoreAudio writes into.
            let status = unsafe {
                AudioObjectGetPropertyData(
                    object,
                    &addr,
                    0,
                    std::ptr::null(),
                    &mut size,
                    &mut value as *mut u32 as *mut c_void,
                )
            };
            (status == NO_ERROR).then_some(value)
        }

        let Some(device) = read_u32(SYSTEM_OBJECT, DEFAULT_OUTPUT_DEVICE).filter(|d| *d != 0) else {
            return true;
        };
        read_u32(device, TRANSPORT_TYPE).map(|t| t == TRANSPORT_BUILTIN).unwrap_or(true)
    }

    /// Sense the environment for one decision.
    ///
    /// Deliberately read fresh rather than from a cache: the microphone check is the safety rule,
    /// and a cached "no mic" from a second ago is exactly the case that puts a chime into a
    /// meeting that just started. It costs three CoreAudio property reads on a path that fires at
    /// most once every two seconds (`sound::MIN_GAP_MS`), and never during notch expansion unless
    /// the user opted into `Full`.
    fn sense(settings: Settings, last_played: Option<Instant>) -> Env {
        Env {
            settings,
            os_ui_sounds_enabled: os_ui_sounds_enabled(),
            mic_in_use: crate::mic::input_in_use(),
            output_is_builtin_speaker: output_is_builtin_speaker(),
            now_min: now_local_min(),
            ms_since_same_sound: last_played.map(|t| t.elapsed().as_millis() as u64),
        }
    }

    // ── playing ─────────────────────────────────────────────────────────────────────────────

    /// Play `cue` if the policy allows it. Never blocks the caller: the decision is cheap and the
    /// playback hops to the main thread.
    pub fn play(app: &AppHandle, cue: Cue) {
        let (settings, last) = {
            let Ok(g) = STATE.lock() else { return };
            match g.as_ref() {
                Some(s) => (s.settings, s.last_played.get(cue.asset()).copied()),
                None => (Settings::default(), None),
            }
        };
        let env = sense(settings, last);
        match sound::should_play(cue, &env) {
            Verdict::Play(asset) => {
                if let Ok(mut g) = STATE.lock() {
                    if let Some(s) = g.as_mut() {
                        s.last_played.insert(asset, Instant::now());
                    }
                }
                eprintln!("[sound] {} → {asset}", cue.id());
                let _ = app.run_on_main_thread(move || emit(asset));
            }
            Verdict::Silent(reason) => {
                // Only the two categories that exist for the user's benefit are worth a line: for
                // Ack/Ready, silence is the steady state and logging it is noise.
                if matches!(cue.category(), Category::Ask | Category::Fail) {
                    eprintln!("[sound] {} silent: {}", cue.id(), reason.tag());
                }
            }
        }
    }

    /// Play the asset now. Main thread only.
    fn emit(asset: &'static str) {
        PLAYERS.with(|players| {
            let Ok(players) = players.try_borrow() else { return };
            let Some(sound) = players.get(asset) else {
                eprintln!("[sound] asset {asset} not loaded");
                return;
            };
            // A cue retriggered while still ringing has to be rewound; `play` alone would be
            // ignored by an already-playing NSSound.
            unsafe {
                if sound.isPlaying() {
                    let _ = sound.stop();
                }
                let _ = sound.play();
            }
        });
    }

    // ── commands ────────────────────────────────────────────────────────────────────────────

    #[tauri::command]
    pub fn get_sound_settings() -> Settings {
        settings()
    }

    #[tauri::command]
    pub fn set_sound_pref(pref: String, app: AppHandle) -> Result<Settings, String> {
        let pref = Pref::from_tag(&pref);
        update(&app, |s| s.pref = pref)
    }

    #[tauri::command]
    pub fn set_sound_startup(enabled: bool, app: AppHandle) -> Result<Settings, String> {
        update(&app, |s| s.startup_sound = enabled)
    }

    /// `start`/`end` are minutes since local midnight, clamped to a real time of day so a bad
    /// value cannot silence the app forever.
    #[tauri::command]
    pub fn set_sound_quiet_hours(
        enabled: bool,
        start_min: u16,
        end_min: u16,
        app: AppHandle,
    ) -> Result<Settings, String> {
        let clamp = |m: u16| m.min(23 * 60 + 59);
        update(&app, |s| {
            s.quiet_hours = QuietHours {
                enabled,
                start_min: clamp(start_min),
                end_min: clamp(end_min),
            }
        })
    }

    /// Settings preview. The user pressed a button, so the preference tiers and quiet hours do not
    /// apply — but the microphone rule and the system setting still do, because those are about
    /// other people and about the OS, not about taste.
    #[tauri::command]
    pub fn preview_sound_cue(asset: String, app: AppHandle) -> Result<bool, String> {
        let Some(asset) = sound::ASSETS.into_iter().find(|a| *a == asset.as_str()) else {
            return Err("unknown cue".into());
        };
        if !os_ui_sounds_enabled() {
            return Ok(false);
        }
        if crate::mic::input_in_use() && output_is_builtin_speaker() {
            return Ok(false);
        }
        let _ = app.run_on_main_thread(move || emit(asset));
        Ok(true)
    }
}
