//! Native ambient music for onboarding. Rust owns playback; a WKWebView never does.

const START_VOLUME: f32 = 0.50;
const SETTLED_VOLUME: f32 = 0.40;
const FADE_STEPS: u8 = 10;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MusicStart {
    pub generation: u64,
    pub volume: f32,
}

#[derive(Debug, Default)]
pub struct MusicController {
    generation: u64,
    active: bool,
    muted: bool,
    fade_step: u8,
    paused_for_voice: bool,
}

impl MusicController {
    pub fn start(&mut self, muted: bool) -> MusicStart {
        self.generation = self.generation.wrapping_add(1);
        self.active = true;
        self.muted = muted;
        self.fade_step = 0;
        self.paused_for_voice = false;
        MusicStart {
            generation: self.generation,
            volume: START_VOLUME,
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
    pub fn set_muted(&mut self, muted: bool) -> bool {
        if !self.active || self.muted == muted {
            return false;
        }
        self.muted = muted;
        true
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
    pub fn set_voice_capture(&mut self, active: bool) -> bool {
        if !self.active || self.muted || self.paused_for_voice == active {
            return false;
        }
        self.paused_for_voice = active;
        true
    }
    pub fn stop(&mut self) -> bool {
        if !self.active {
            return false;
        }
        self.active = false;
        self.fade_step = FADE_STEPS;
        self.paused_for_voice = false;
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

#[cfg(target_os = "macos")]
pub mod mac {
    use super::MusicController;
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

    struct RuntimeState {
        controller: MusicController,
        player: Option<usize>,
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
        let pointer = state.player.take()? as *mut AnyObject;
        // SAFETY: player entries originate from `Retained::into_raw` and are consumed once.
        unsafe { Retained::from_raw(pointer) }
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

    fn schedule_player_monitor(app: AppHandle, generation: u64) {
        std::thread::spawn(move || loop {
            std::thread::sleep(FADE_INTERVAL);
            let callback_app = app.clone();
            let (sender, receiver) = std::sync::mpsc::sync_channel(1);
            if app
                .run_on_main_thread(move || {
                    let _ = sender.send(apply_tick(&callback_app, generation));
                })
                .is_err()
            {
                return;
            }
            if receiver.recv().unwrap_or(false) == false {
                return;
            }
        });
    }

    fn apply_tick(app: &AppHandle, generation: u64) -> bool {
        let Some(runtime) = app.try_state::<OnboardingMusic>() else {
            return false;
        };
        let voice_active = crate::voice_session::mac::capture_active();
        let Ok(mut state) = runtime.0.lock() else {
            return false;
        };
        if !state.controller.active() || state.controller.generation() != generation {
            return false;
        }
        if state.controller.set_voice_capture(voice_active) {
            if let Some(pointer) = state.player {
                let player = unsafe { &*(pointer as *const AnyObject) };
                if voice_active {
                    pause(player);
                } else if !play(player) {
                    state.controller.playback_failed(generation);
                    if let Some(player) = take_player(&mut state) {
                        stop_player(player);
                    }
                    return false;
                }
            }
        }
        if let Some(volume) = state.controller.fade_tick(generation) {
            if let Some(pointer) = state.player {
                set_volume(unsafe { &*(pointer as *const AnyObject) }, volume);
            }
        }
        true
    }

    /// Start one looping AVAudioPlayer. Decoder failures are silent and never block onboarding.
    pub fn start(app: &AppHandle, muted: bool) {
        let Some(runtime) = app.try_state::<OnboardingMusic>() else {
            return;
        };
        let Ok(mut state) = runtime.0.lock() else {
            return;
        };
        if let Some(player) = take_player(&mut state) {
            stop_player(player);
        }
        let start = state.controller.start(muted);
        let voice_active = crate::voice_session::mac::capture_active();
        if voice_active {
            state.controller.set_voice_capture(true);
        }
        let Some(player) = create_player(start.volume, !muted && !voice_active) else {
            state.controller.playback_failed(start.generation);
            return;
        };
        state.player = Some(Retained::into_raw(player) as usize);
        drop(state);
        schedule_player_monitor(app.clone(), start.generation);
    }

    /// Apply already-persisted Mute state on AppKit's main thread.
    fn set_muted_main(app: &AppHandle, muted: bool) {
        let Some(runtime) = app.try_state::<OnboardingMusic>() else {
            return;
        };
        let Ok(mut state) = runtime.0.lock() else {
            return;
        };
        if !state.controller.set_muted(muted) {
            return;
        }
        let Some(pointer) = state.player else {
            return;
        };
        let player = unsafe { &*(pointer as *const AnyObject) };
        if muted {
            pause(player);
        } else if !crate::voice_session::mac::capture_active() && !play(player) {
            let generation = state.controller.generation();
            state.controller.playback_failed(generation);
            if let Some(player) = take_player(&mut state) {
                stop_player(player);
            }
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
    use super::{MusicController, SETTLED_VOLUME, START_VOLUME};
    #[test]
    fn fade_starts_at_half_then_settles_monotonically_at_forty_percent() {
        let mut controller = MusicController::default();
        let start = controller.start(false);
        let volumes = (0..10)
            .filter_map(|_| controller.fade_tick(start.generation))
            .collect::<Vec<_>>();
        assert_eq!(start.volume, START_VOLUME);
        assert_eq!(volumes.last().copied(), Some(SETTLED_VOLUME));
        assert!(volumes.windows(2).all(|pair| pair[0] >= pair[1]));
    }
    #[test]
    fn stale_fade_generation_cannot_touch_replacement_player() {
        let mut controller = MusicController::default();
        let first = controller.start(false);
        let second = controller.start(false);
        assert_eq!(controller.fade_tick(first.generation), None);
        assert!(controller.fade_tick(second.generation).is_some());
    }
    #[test]
    fn replacement_keeps_one_logical_player() {
        let mut controller = MusicController::default();
        controller.start(false);
        controller.start(false);
        assert_eq!(controller.player_count(), 1);
    }

    #[test]
    fn mute_is_immediate_and_unmute_keeps_the_existing_player() {
        let mut controller = MusicController::default();
        let start = controller.start(false);
        assert!(controller.set_muted(true));
        assert_eq!(controller.fade_tick(start.generation), None);
        assert!(controller.set_muted(false));
        assert_eq!(controller.player_count(), 1);
    }
    #[test]
    fn stop_invalidates_delayed_work_and_releases_player_ownership() {
        let mut controller = MusicController::default();
        let start = controller.start(false);
        assert!(controller.stop());
        assert_eq!(controller.player_count(), 0);
        assert_eq!(controller.fade_tick(start.generation), None);
    }
    #[test]
    fn playback_failure_is_nonblocking_and_stale_failure_is_ignored() {
        let mut controller = MusicController::default();
        let first = controller.start(false);
        let second = controller.start(false);
        assert!(!controller.playback_failed(first.generation));
        assert!(controller.active());
        assert!(controller.playback_failed(second.generation));
        assert!(!controller.active());
    }
}
