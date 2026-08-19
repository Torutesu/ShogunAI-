//! Native ambient music for onboarding. Rust owns playback; a WKWebView never does.

const START_VOLUME: f32 = 0.50;
const SETTLED_VOLUME: f32 = 0.40;
const FADE_STEPS: u8 = 10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FadeScheduler {
    generation: u64,
    lease: u64,
}

#[derive(Debug, Default)]
pub struct MuteApplyGate(std::sync::atomic::AtomicU8);

impl MuteApplyGate {
    const PENDING: u8 = 0;
    const APPLYING: u8 = 1;
    const CANCELLED: u8 = 2;
    const COMPLETE: u8 = 3;

    pub fn claim(&self) -> bool {
        self.0
            .compare_exchange(
                Self::PENDING,
                Self::APPLYING,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_ok()
    }
    pub fn cancel_pending(&self) -> bool {
        self.0
            .compare_exchange(
                Self::PENDING,
                Self::CANCELLED,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_ok()
    }
    pub fn finish(&self) {
        self.0
            .store(Self::COMPLETE, std::sync::atomic::Ordering::Release);
    }
}

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
    fade_scheduler: Option<FadeScheduler>,
    next_fade_lease: u64,
}

impl MusicController {
    pub fn start(&mut self, muted: bool, voice_capture: Option<bool>) -> MusicStart {
        self.generation = self.generation.wrapping_add(1);
        self.active = true;
        self.muted = muted;
        self.fade_step = 0;
        self.fade_scheduler = None;
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
    fn fade_pending(&self) -> bool {
        self.active && self.fade_step < FADE_STEPS
    }
    pub fn should_schedule_fade(&self) -> bool {
        self.fade_pending() && self.audible()
    }
    pub fn claim_fade_scheduler(&mut self) -> Option<FadeScheduler> {
        if self.should_schedule_fade() && self.fade_scheduler.is_none() {
            self.next_fade_lease = self.next_fade_lease.wrapping_add(1);
            let scheduler = FadeScheduler {
                generation: self.generation,
                lease: self.next_fade_lease,
            };
            self.fade_scheduler = Some(scheduler);
            return Some(scheduler);
        }
        None
    }
    pub fn owns_fade_scheduler(&self, scheduler: FadeScheduler) -> bool {
        self.fade_scheduler == Some(scheduler)
    }
    pub fn finish_fade_scheduler(&mut self, scheduler: FadeScheduler) {
        if self.fade_scheduler == Some(scheduler) {
            self.fade_scheduler = None;
        }
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
            || !self.audible()
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
        self.fade_scheduler = None;
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
    use super::{FadeScheduler, MusicController, MuteApplyGate, PlaybackAction};
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send, MainThreadMarker};
    use objc2_foundation::NSData;
    use std::sync::{Arc, Mutex};
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

    fn schedule_fade(app: AppHandle, scheduler: FadeScheduler) {
        std::thread::spawn(move || loop {
            std::thread::sleep(FADE_INTERVAL);
            let callback_app = app.clone();
            let (sender, receiver) = std::sync::mpsc::sync_channel(1);
            if app
                .run_on_main_thread(move || {
                    let _ = sender.send(apply_fade_tick(&callback_app, scheduler));
                })
                .is_err()
            {
                release_fade_scheduler(&app, scheduler);
                return;
            }
            let Ok(continue_fade) = receiver.recv_timeout(Duration::from_secs(1)) else {
                release_fade_scheduler(&app, scheduler);
                return;
            };
            if !continue_fade {
                return;
            }
        });
    }

    fn release_fade_scheduler(app: &AppHandle, scheduler: FadeScheduler) {
        let Some(runtime) = app.try_state::<OnboardingMusic>() else {
            return;
        };
        // Only controller bookkeeping happens off-main. AVAudioPlayer remains main-thread-only.
        if let Ok(mut state) = runtime.0.lock() {
            state.controller.finish_fade_scheduler(scheduler);
        };
    }

    fn apply_fade_tick(app: &AppHandle, scheduler: FadeScheduler) -> bool {
        let Some(runtime) = app.try_state::<OnboardingMusic>() else {
            return false;
        };
        let Ok(mut state) = runtime.0.lock() else {
            return false;
        };
        if !state.controller.active()
            || state.controller.generation() != scheduler.generation
            || !state.controller.owns_fade_scheduler(scheduler)
        {
            return false;
        }
        if let Some(volume) = state.controller.fade_tick(scheduler.generation) {
            if let Some(player) = state.player.as_ref() {
                set_volume(&player.0, volume);
            }
        }
        let continue_fade = state.controller.should_schedule_fade();
        if !continue_fade {
            state.controller.finish_fade_scheduler(scheduler);
        }
        continue_fade
    }

    fn apply_playback_action(state: &mut RuntimeState, action: PlaybackAction) -> Result<(), ()> {
        let Some(player) = state.player.as_ref() else {
            return Ok(());
        };
        match action {
            PlaybackAction::Pause => {
                pause(&player.0);
                Ok(())
            }
            PlaybackAction::Play if !play(&player.0) => {
                let generation = state.controller.generation();
                state.controller.playback_failed(generation);
                if let Some(player) = take_player(state) {
                    stop_player(player);
                }
                Err(())
            }
            PlaybackAction::Play => Ok(()),
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
        let schedule = state.controller.claim_fade_scheduler();
        drop(state);
        if let Some(scheduler) = schedule {
            schedule_fade(app.clone(), scheduler);
        }
    }

    fn voice_capture_changed_main(
        app: &AppHandle,
        voice_capture: Option<bool>,
    ) -> Result<Option<FadeScheduler>, ()> {
        let Some(runtime) = app.try_state::<OnboardingMusic>() else {
            return Ok(None);
        };
        let mut state = runtime.0.lock().map_err(|_| ())?;
        if let Some(action) = state.controller.set_voice_capture(voice_capture) {
            apply_playback_action(&mut state, action)?;
        }
        if voice_capture.unwrap_or(true) && state.controller.audible() {
            return Err(());
        }
        Ok(state.controller.claim_fade_scheduler())
    }

    /// Voice owns capture truth. `None` means its mutex is poisoned and therefore pauses music.
    pub fn voice_capture_changed(app: &AppHandle, voice_capture: Option<bool>) -> Result<(), ()> {
        if MainThreadMarker::new().is_some() {
            if let Some(scheduler) = voice_capture_changed_main(app, voice_capture)? {
                schedule_fade(app.clone(), scheduler);
            }
            return Ok(());
        }
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let callback_app = app.clone();
        if app
            .run_on_main_thread(move || {
                let _ = sender.send(voice_capture_changed_main(&callback_app, voice_capture));
            })
            .is_err()
        {
            return Err(());
        }
        let scheduler = receiver
            .recv_timeout(Duration::from_secs(1))
            .map_err(|_| ())??;
        if let Some(scheduler) = scheduler {
            schedule_fade(app.clone(), scheduler);
        }
        Ok(())
    }

    /// Apply persisted Mute state on AppKit's main thread.
    fn set_muted_main(app: &AppHandle, muted: bool) -> Result<Option<FadeScheduler>, String> {
        let Some(runtime) = app.try_state::<OnboardingMusic>() else {
            return Ok(None);
        };
        let mut state = runtime
            .0
            .lock()
            .map_err(|_| "onboarding music unavailable".to_owned())?;
        if let Some(action) = state.controller.set_muted(muted) {
            apply_playback_action(&mut state, action)
                .map_err(|_| "onboarding music playback unavailable".to_owned())?;
        }
        Ok(state.controller.claim_fade_scheduler())
    }

    /// Apply Mute state synchronously. A caller never receives success before AppKit applies it.
    pub fn set_muted(app: &AppHandle, muted: bool) -> Result<(), String> {
        if MainThreadMarker::new().is_some() {
            if let Some(scheduler) = set_muted_main(app, muted)? {
                schedule_fade(app.clone(), scheduler);
            }
            return Ok(());
        }
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let gate = Arc::new(MuteApplyGate::default());
        let callback_gate = Arc::clone(&gate);
        let callback_app = app.clone();
        if app
            .run_on_main_thread(move || {
                if !callback_gate.claim() {
                    let _ = sender.send(Err("onboarding music mute request cancelled".to_owned()));
                    return;
                }
                let result = set_muted_main(&callback_app, muted);
                callback_gate.finish();
                let _ = sender.send(result);
            })
            .is_err()
        {
            return Err("onboarding music main-thread queue unavailable".to_owned());
        }
        let scheduler = match receiver.recv_timeout(Duration::from_secs(1)) {
            Ok(result) => result?,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) if gate.cancel_pending() => {
                return Err("onboarding music main-thread queue timed out".to_owned());
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => receiver
                .recv()
                .map_err(|_| "onboarding music main-thread acknowledgement lost".to_owned())??,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err("onboarding music main-thread acknowledgement lost".to_owned());
            }
        };
        if let Some(scheduler) = scheduler {
            schedule_fade(app.clone(), scheduler);
        }
        Ok(())
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
    use super::{
        MusicController, MuteApplyGate, PlaybackAction, FADE_STEPS, SETTLED_VOLUME, START_VOLUME,
    };
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
    fn mute_during_fade_resumes_remaining_steps_and_settles_at_forty_percent() {
        let mut controller = MusicController::default();
        let start = controller.start(false, Some(false));
        assert_eq!(controller.fade_tick(start.generation), Some(0.49));
        assert_eq!(controller.set_muted(true), Some(PlaybackAction::Pause));
        assert_eq!(controller.fade_tick(start.generation), None);
        assert_eq!(controller.set_muted(false), Some(PlaybackAction::Play));
        let volumes = (0..FADE_STEPS)
            .filter_map(|_| controller.fade_tick(start.generation))
            .collect::<Vec<_>>();
        assert_eq!(volumes.len(), 9);
        assert_eq!(volumes.last().copied(), Some(SETTLED_VOLUME));
        assert_eq!(controller.fade_tick(start.generation), None);
    }
    #[test]
    fn persisted_muted_start_unmute_runs_the_full_remaining_fade() {
        let mut controller = MusicController::default();
        let start = controller.start(true, Some(false));
        assert!(!start.should_play);
        assert_eq!(controller.set_muted(false), Some(PlaybackAction::Play));
        let volumes = (0..FADE_STEPS)
            .filter_map(|_| controller.fade_tick(start.generation))
            .collect::<Vec<_>>();
        assert_eq!(volumes.len(), usize::from(FADE_STEPS));
        assert_eq!(volumes.last().copied(), Some(SETTLED_VOLUME));
    }
    #[test]
    fn rapid_mute_unmute_keeps_one_claimed_fade_scheduler() {
        let mut controller = MusicController::default();
        let start = controller.start(false, Some(false));
        let first = controller.claim_fade_scheduler().expect("first scheduler");
        assert_eq!(first.generation, start.generation);
        assert_eq!(controller.set_muted(true), Some(PlaybackAction::Pause));
        assert_eq!(controller.claim_fade_scheduler(), None);
        assert_eq!(controller.set_muted(false), Some(PlaybackAction::Play));
        // The in-flight scheduler owns this generation until its stopped callback clears it.
        assert_eq!(controller.claim_fade_scheduler(), None);
        controller.finish_fade_scheduler(first);
        let replacement = controller
            .claim_fade_scheduler()
            .expect("replacement scheduler");
        assert_eq!(replacement.generation, start.generation);
        // A late timeout callback from the first scheduler cannot release the new lease.
        controller.finish_fade_scheduler(first);
        assert_eq!(controller.claim_fade_scheduler(), None);
    }
    #[test]
    fn timed_out_mute_request_cancels_before_a_late_main_callback_can_apply() {
        let gate = MuteApplyGate::default();
        assert!(gate.cancel_pending());
        assert!(!gate.claim());
    }
    #[test]
    fn claimed_mute_request_forces_caller_to_wait_for_acknowledgement() {
        let gate = MuteApplyGate::default();
        assert!(gate.claim());
        assert!(!gate.cancel_pending());
        gate.finish();
    }
    #[test]
    fn fade_queue_failure_or_ack_timeout_releases_only_its_own_lease() {
        let mut controller = MusicController::default();
        controller.start(false, Some(false));
        let first = controller.claim_fade_scheduler().expect("first scheduler");
        // Queue failure releases first so a later Unmute can claim again.
        controller.finish_fade_scheduler(first);
        let second = controller.claim_fade_scheduler().expect("second scheduler");
        // A late first callback is stale and cannot clear second.
        controller.finish_fade_scheduler(first);
        assert_eq!(controller.claim_fade_scheduler(), None);
        // Ack timeout releases second, then a later retry can claim a fresh lease.
        controller.finish_fade_scheduler(second);
        assert!(controller.claim_fade_scheduler().is_some());
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
