//! Native ambient music for onboarding. Rust owns playback; a WKWebView never does.

const START_VOLUME: f32 = 0.50;
const SETTLED_VOLUME: f32 = 0.40;
const FADE_STEPS: u8 = 10;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MusicStart {
    pub generation: u64,
    pub volume: f32,
    pub should_play: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackAction {
    Pause,
    Play,
}

#[derive(Debug, Default)]
pub struct MusicController {
    generation: u64,
    active: bool,
    muted: bool,
    fade_step: u8,
    voice_capture: bool,
}

impl MusicController {
    pub fn start(&mut self, muted: bool, voice_capture: Option<bool>) -> MusicStart {
        self.generation = self.generation.wrapping_add(1);
        self.active = true;
        self.muted = muted;
        self.fade_step = 0;
        // A poisoned voice lane is a hot-mic safety failure, never proof that capture is idle.
        self.voice_capture = voice_capture.unwrap_or(true);
        MusicStart {
            generation: self.generation,
            volume: START_VOLUME,
            should_play: self.audible(),
        }
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn active(&self) -> bool {
        self.active
    }
    #[cfg(test)]
    pub fn player_count(&self) -> usize {
        usize::from(self.active)
    }
    fn audible(&self) -> bool {
        self.active && !self.muted && !self.voice_capture
    }
    pub fn set_muted(&mut self, muted: bool) -> Option<PlaybackAction> {
        if !self.active || self.muted == muted {
            return None;
        }
        let was_audible = self.audible();
        self.muted = muted;
        playback_transition(was_audible, self.audible())
    }
    pub fn fade_tick(&mut self, generation: u64) -> Option<f32> {
        if !self.active
            || self.muted
            || generation != self.generation
            || self.fade_step >= FADE_STEPS
        {
            return None;
        }
        self.fade_step += 1;
        Some(
            START_VOLUME
                - (START_VOLUME - SETTLED_VOLUME) * f32::from(self.fade_step)
                    / f32::from(FADE_STEPS),
        )
    }
    pub fn set_voice_capture(&mut self, voice_capture: Option<bool>) -> Option<PlaybackAction> {
        if !self.active {
            return None;
        }
        let voice_capture = voice_capture.unwrap_or(true);
        if self.voice_capture == voice_capture {
            return None;
        }
        let was_audible = self.audible();
        self.voice_capture = voice_capture;
        playback_transition(was_audible, self.audible())
    }
    pub fn stop(&mut self) -> bool {
        if !self.active {
            return false;
        }
        self.active = false;
        self.fade_step = FADE_STEPS;
        self.voice_capture = false;
        self.generation = self.generation.wrapping_add(1);
        true
    }
    pub fn playback_failed(&mut self, generation: u64) -> bool {
        if !self.active || generation != self.generation {
            return false;
        }
        self.stop()
    }
}

fn playback_transition(was_audible: bool, audible: bool) -> Option<PlaybackAction> {
    match (was_audible, audible) {
        (true, false) => Some(PlaybackAction::Pause),
        (false, true) => Some(PlaybackAction::Play),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
pub mod mac {
    use super::{MusicController, PlaybackAction, FADE_STEPS};
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send, MainThreadMarker};
    use objc2_foundation::NSData;
    use std::sync::Mutex;
    use std::time::Duration;
    use tauri::{AppHandle, Manager};

    const FADE_INTERVAL: Duration = Duration::from_millis(90);
    const MUSIC_BYTES: &[u8] =
        include_bytes!("../../src/assets/onboarding/audio/yoiyami_core_theme.mp3");

    /// `AVAudioPlayer` is only created, read, and released from AppKit's main thread.
    /// Tauri requires managed state to be Send, so this wrapper documents that boundary
    /// without erasing ownership into an integer pointer.
    struct MainThreadPlayer(Retained<AnyObject>);
    // SAFETY: every access is routed through `run_on_main_thread` or a main-thread caller.
    unsafe impl Send for MainThreadPlayer {}

    struct RuntimeState {
        controller: MusicController,
        player: Option<MainThreadPlayer>,
    }
    impl Default for RuntimeState {
        fn default() -> Self {
            Self {
                controller: MusicController::default(),
                player: None,
            }
        }
    }
    pub struct OnboardingMusic(Mutex<RuntimeState>);
    impl Default for OnboardingMusic {
        fn default() -> Self {
            Self(Mutex::new(RuntimeState::default()))
        }
    }

    fn take_player(state: &mut RuntimeState) -> Option<Retained<AnyObject>> {
        state.player.take().map(|player| player.0)
    }
    fn stop_player(player: Retained<AnyObject>) {
        unsafe {
            let _: () = msg_send![Retained::as_ptr(&player), stop];
        }
    }
    fn set_volume(player: &AnyObject, volume: f32) {
        unsafe {
            let _: () = msg_send![player, setVolume: volume];
        }
    }
    fn pause(player: &AnyObject) {
        unsafe {
            let _: () = msg_send![player, pause];
        }
    }
    fn play(player: &AnyObject) -> bool {
        unsafe { msg_send![player, play] }
    }

    fn create_player(volume: f32, should_play: bool) -> Option<Retained<AnyObject>> {
        unsafe {
            let data = NSData::dataWithBytes_length(MUSIC_BYTES.as_ptr().cast(), MUSIC_BYTES.len());
            let allocated: *mut AnyObject = msg_send![class!(AVAudioPlayer), alloc];
            if allocated.is_null() {
                return None;
            }
            let mut error: *mut AnyObject = std::ptr::null_mut();
            let raw: *mut AnyObject = msg_send![allocated, initWithData: &*data, error: &mut error];
            let player = Retained::from_raw(raw)?;
            let _: () = msg_send![Retained::as_ptr(&player), setNumberOfLoops: -1_i64];
            set_volume(&player, volume);
            (!should_play || play(&player)).then_some(player)
        }
    }

    fn schedule_fade(app: AppHandle, generation: u64) {
        std::thread::spawn(move || {
            for _ in 0..FADE_STEPS {
                std::thread::sleep(FADE_INTERVAL);
                let callback_app = app.clone();
                if app
                    .run_on_main_thread(move || {
                        apply_fade_tick(&callback_app, generation);
                    })
                    .is_err()
                {
                    return;
                }
            }
        });
    }

    fn apply_fade_tick(app: &AppHandle, generation: u64) {
        let Some(runtime) = app.try_state::<OnboardingMusic>() else {
            return;
        };
        let Ok(mut state) = runtime.0.lock() else {
            return;
        };
        if !state.controller.active() || state.controller.generation() != generation {
            return;
        }
        if let Some(volume) = state.controller.fade_tick(generation) {
            if let Some(player) = state.player.as_ref() {
                set_volume(&player.0, volume);
            }
        }
    }

    fn apply_playback_action(state: &mut RuntimeState, action: PlaybackAction) {
        let Some(player) = state.player.as_ref() else {
            return;
        };
        match action {
            PlaybackAction::Pause => pause(&player.0),
            PlaybackAction::Play if !play(&player.0) => {
                let generation = state.controller.generation();
                state.controller.playback_failed(generation);
                if let Some(player) = take_player(state) {
                    stop_player(player);
                }
            }
            PlaybackAction::Play => {}
        }
    }

    /// Start one looping AVAudioPlayer. Decoder failures are silent and never block onboarding.
    pub fn start(app: &AppHandle, muted: bool, voice_capture: Option<bool>) {
        let Some(runtime) = app.try_state::<OnboardingMusic>() else {
            return;
        };
        let Ok(mut state) = runtime.0.lock() else {
            return;
        };
        if let Some(player) = take_player(&mut state) {
            stop_player(player);
        }
        let start = state.controller.start(muted, voice_capture);
        let Some(player) = create_player(start.volume, start.should_play) else {
            state.controller.playback_failed(start.generation);
            return;
        };
        state.player = Some(MainThreadPlayer(player));
        drop(state);
        schedule_fade(app.clone(), start.generation);
    }

    fn voice_capture_changed_main(app: &AppHandle, voice_capture: Option<bool>) {
        let Some(runtime) = app.try_state::<OnboardingMusic>() else {
            return;
        };
        let Ok(mut state) = runtime.0.lock() else {
            return;
        };
        if let Some(action) = state.controller.set_voice_capture(voice_capture) {
            apply_playback_action(&mut state, action);
        }
    }

    /// Voice owns capture truth. `None` means its mutex is poisoned and therefore pauses music.
    pub fn voice_capture_changed(app: &AppHandle, voice_capture: Option<bool>) {
        if MainThreadMarker::new().is_some() {
            voice_capture_changed_main(app, voice_capture);
            return;
        }
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let callback_app = app.clone();
        if app
            .run_on_main_thread(move || {
                voice_capture_changed_main(&callback_app, voice_capture);
                let _ = sender.send(());
            })
            .is_ok()
        {
            let _ = receiver.recv_timeout(Duration::from_secs(1));
        }
    }

    /// Apply already-persisted Mute state on AppKit's main thread.
    fn set_muted_main(app: &AppHandle, muted: bool) {
        let Some(runtime) = app.try_state::<OnboardingMusic>() else {
            return;
        };
        let Ok(mut state) = runtime.0.lock() else {
            return;
        };
        if let Some(action) = state.controller.set_muted(muted) {
            apply_playback_action(&mut state, action);
        }
    }

    /// Apply already-persisted Mute state; state persistence belongs to onboarding Store.
    pub fn set_muted(app: &AppHandle, muted: bool) {
        if MainThreadMarker::new().is_some() {
            set_muted_main(app, muted);
            return;
        }
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let callback_app = app.clone();
        if app
            .run_on_main_thread(move || {
                set_muted_main(&callback_app, muted);
                let _ = sender.send(());
            })
            .is_ok()
        {
            let _ = receiver.recv_timeout(Duration::from_secs(1));
        }
    }

    /// Idempotent cleanup for completion, close, restart, replacement, and app exit.
    fn stop_main(app: &AppHandle) {
        let Some(runtime) = app.try_state::<OnboardingMusic>() else {
            return;
        };
        let Ok(mut state) = runtime.0.lock() else {
            return;
        };
        state.controller.stop();
        if let Some(player) = take_player(&mut state) {
            stop_player(player);
        }
    }

    /// Idempotent cleanup for completion, close, restart, replacement, and app exit.
    pub fn stop(app: &AppHandle) {
        if MainThreadMarker::new().is_some() {
            stop_main(app);
            return;
        }
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let callback_app = app.clone();
        if app
            .run_on_main_thread(move || {
                stop_main(&callback_app);
                let _ = sender.send(());
            })
            .is_ok()
        {
            let _ = receiver.recv_timeout(Duration::from_secs(1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MusicController, PlaybackAction, SETTLED_VOLUME, START_VOLUME};
    #[test]
    fn fade_starts_at_half_then_settles_monotonically_at_forty_percent() {
        let mut controller = MusicController::default();
        let start = controller.start(false, Some(false));
        let volumes = (0..10)
            .filter_map(|_| controller.fade_tick(start.generation))
            .collect::<Vec<_>>();
        assert_eq!(start.volume, START_VOLUME);
        assert_eq!(volumes.len(), 10);
        assert_eq!(volumes.last().copied(), Some(SETTLED_VOLUME));
        assert!(volumes.windows(2).all(|pair| pair[0] >= pair[1]));
        assert_eq!(controller.fade_tick(start.generation), None);
    }
    #[test]
    fn stale_fade_generation_cannot_touch_replacement_player() {
        let mut controller = MusicController::default();
        let first = controller.start(false, Some(false));
        let second = controller.start(false, Some(false));
        assert_eq!(controller.fade_tick(first.generation), None);
        assert!(controller.fade_tick(second.generation).is_some());
    }
    #[test]
    fn display_generation_replacement_stops_old_player_and_keeps_one_logical_player() {
        let mut controller = MusicController::default();
        let first = controller.start(false, Some(false));
        let replacement = controller.start(false, Some(false));
        assert_eq!(controller.player_count(), 1);
        assert_eq!(controller.fade_tick(first.generation), None);
        assert!(replacement.should_play);
    }

    #[test]
    fn mute_is_immediate_and_unmute_keeps_the_existing_player() {
        let mut controller = MusicController::default();
        let start = controller.start(false, Some(false));
        assert_eq!(controller.set_muted(true), Some(PlaybackAction::Pause));
        assert_eq!(controller.fade_tick(start.generation), None);
        assert_eq!(controller.set_muted(false), Some(PlaybackAction::Play));
        assert_eq!(controller.player_count(), 1);
    }
    #[test]
    fn stop_invalidates_delayed_work_and_releases_player_ownership() {
        let mut controller = MusicController::default();
        let start = controller.start(false, Some(false));
        assert!(controller.stop());
        assert_eq!(controller.player_count(), 0);
        assert_eq!(controller.fade_tick(start.generation), None);
    }
    #[test]
    fn playback_failure_is_nonblocking_and_stale_failure_is_ignored() {
        let mut controller = MusicController::default();
        let first = controller.start(false, Some(false));
        let second = controller.start(false, Some(false));
        assert!(!controller.playback_failed(first.generation));
        assert!(controller.active());
        assert!(controller.playback_failed(second.generation));
        assert!(!controller.active());
    }

    #[test]
    fn poisoned_voice_state_fails_closed_and_explicit_capture_transitions_pause_music() {
        let mut controller = MusicController::default();
        let start = controller.start(false, None);
        assert!(!start.should_play);
        assert_eq!(
            controller.set_voice_capture(Some(false)),
            Some(PlaybackAction::Play)
        );
        assert_eq!(
            controller.set_voice_capture(Some(true)),
            Some(PlaybackAction::Pause)
        );
    }
}
