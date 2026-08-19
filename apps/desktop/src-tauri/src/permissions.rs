//! Typed, side-effect-free permission status and one generation-owned refresh coordinator.

use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessibilityState {
    Granted,
    NotGranted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MicrophoneState {
    NotDetermined,
    Denied,
    Restricted,
    Granted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenRecordingState {
    NotGranted,
    Granted,
    RestartRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionReason {
    ScreenRecordingRestartRequired,
    ScreenRecordingSettingsRepairPending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScreenRequestResult {
    AlreadyEffective,
    PromptGranted,
    SettingsOpened,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawPermissionStatus {
    pub accessibility: bool,
    pub microphone: MicrophoneState,
    pub screen_recording_effective: bool,
}

pub trait PermissionProvider: Send + Sync + 'static {
    fn status(&self) -> RawPermissionStatus;
    fn request_accessibility(&self) -> Result<(), String>;
    fn request_microphone(&self, finished: Box<dyn FnOnce() + Send>) -> Result<(), String>;
    fn request_screen_recording(&self) -> Result<ScreenRequestResult, String>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct PermissionSnapshot {
    // Compatibility booleans. Typed fields below carry denial/restart detail.
    pub accessibility: bool,
    pub microphone: bool,
    pub screen_recording: bool,
    pub all_granted: bool,
    pub accessibility_state: AccessibilityState,
    pub microphone_state: MicrophoneState,
    pub screen_recording_state: ScreenRecordingState,
    pub all_effective: bool,
    pub reason: Option<PermissionReason>,
    pub revision: u64,
}

impl Default for PermissionSnapshot {
    fn default() -> Self {
        Self {
            accessibility: false,
            microphone: false,
            screen_recording: false,
            all_granted: false,
            accessibility_state: AccessibilityState::NotGranted,
            microphone_state: MicrophoneState::NotDetermined,
            screen_recording_state: ScreenRecordingState::NotGranted,
            all_effective: false,
            reason: None,
            revision: 0,
        }
    }
}

pub struct PermissionCoordinator<P> {
    provider: P,
    latest: PermissionSnapshot,
    initialized: bool,
    active: bool,
    listener_ready: bool,
    generation: u64,
    screen_repair: ScreenRepair,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScreenRepair {
    None,
    RestartRequired,
    SettingsPending,
}

impl<P: PermissionProvider> PermissionCoordinator<P> {
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            latest: PermissionSnapshot::default(),
            initialized: false,
            active: false,
            listener_ready: false,
            generation: 0,
            screen_repair: ScreenRepair::None,
        }
    }

    fn sampled(&self) -> PermissionSnapshot {
        let raw = self.provider.status();
        let accessibility_state = if raw.accessibility {
            AccessibilityState::Granted
        } else {
            AccessibilityState::NotGranted
        };
        let screen_recording_state = match (raw.screen_recording_effective, self.screen_repair) {
            (true, _) => ScreenRecordingState::Granted,
            (false, ScreenRepair::RestartRequired) => ScreenRecordingState::RestartRequired,
            (false, ScreenRepair::None | ScreenRepair::SettingsPending) => {
                ScreenRecordingState::NotGranted
            }
        };
        let accessibility = accessibility_state == AccessibilityState::Granted;
        let microphone = raw.microphone == MicrophoneState::Granted;
        let screen_recording = screen_recording_state == ScreenRecordingState::Granted;
        let all_effective = accessibility && microphone && screen_recording;
        PermissionSnapshot {
            accessibility,
            microphone,
            screen_recording,
            all_granted: all_effective,
            accessibility_state,
            microphone_state: raw.microphone,
            screen_recording_state,
            all_effective,
            reason: match self.screen_repair {
                ScreenRepair::RestartRequired if !raw.screen_recording_effective => {
                    Some(PermissionReason::ScreenRecordingRestartRequired)
                }
                ScreenRepair::SettingsPending if !raw.screen_recording_effective => {
                    Some(PermissionReason::ScreenRecordingSettingsRepairPending)
                }
                ScreenRepair::None
                | ScreenRepair::RestartRequired
                | ScreenRepair::SettingsPending => None,
            },
            revision: self.latest.revision,
        }
    }

    fn refresh(&mut self) -> Option<PermissionSnapshot> {
        let mut sampled = self.sampled();
        if sampled.screen_recording_effective() {
            self.screen_repair = ScreenRepair::None;
            sampled.screen_recording_state = ScreenRecordingState::Granted;
            sampled.reason = None;
        }
        let changed =
            !self.initialized || sampled.without_revision() != self.latest.without_revision();
        if !changed {
            return None;
        }
        sampled.revision = self.latest.revision.saturating_add(1);
        self.latest = sampled;
        self.initialized = true;
        Some(sampled)
    }

    pub fn status(&mut self) -> (PermissionSnapshot, Option<PermissionSnapshot>) {
        let edge = self.refresh();
        (self.latest, edge)
    }

    pub fn start(&mut self) -> (u64, bool) {
        if self.active {
            return (self.generation, false);
        }
        self.generation = self.generation.wrapping_add(1);
        self.active = true;
        self.listener_ready = false;
        let _ = self.refresh();
        (self.generation, true)
    }

    pub fn listener_ready(&mut self) -> Option<PermissionSnapshot> {
        if !self.active || self.listener_ready {
            return None;
        }
        self.listener_ready = true;
        Some(self.latest)
    }

    pub fn stop(&mut self, generation: u64) {
        if self.active && self.generation == generation {
            self.active = false;
            self.listener_ready = false;
            self.generation = self.generation.wrapping_add(1);
        }
    }

    pub fn generation_active(&self, generation: u64) -> bool {
        self.active && self.generation == generation
    }

    pub fn events_enabled(&self) -> bool {
        !self.active || self.listener_ready
    }

    pub fn poll(&mut self, generation: u64) -> Option<PermissionSnapshot> {
        (self.active && self.generation == generation)
            .then(|| self.refresh())
            .flatten()
    }

    pub fn activation_refresh(&mut self) -> Option<PermissionSnapshot> {
        self.refresh()
    }

    pub fn request_finished(
        &mut self,
        screen_request: Option<ScreenRequestResult>,
    ) -> Option<PermissionSnapshot> {
        if let Some(screen_request) = screen_request {
            self.screen_repair = match screen_request {
                ScreenRequestResult::AlreadyEffective => ScreenRepair::None,
                ScreenRequestResult::PromptGranted => ScreenRepair::RestartRequired,
                ScreenRequestResult::SettingsOpened => ScreenRepair::SettingsPending,
            };
        }
        self.refresh()
    }

    pub fn provider(&self) -> &P {
        &self.provider
    }
}

impl PermissionSnapshot {
    fn screen_recording_effective(self) -> bool {
        self.screen_recording_state == ScreenRecordingState::Granted
    }

    fn without_revision(mut self) -> Self {
        self.revision = 0;
        self
    }
}

#[cfg(target_os = "macos")]
pub mod mac {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;

    use tauri::{AppHandle, Emitter, Manager};

    use super::{
        MicrophoneState, PermissionCoordinator, PermissionProvider, PermissionSnapshot,
        RawPermissionStatus, ScreenRequestResult,
    };
    use crate::axcache;

    const AX_SETTINGS_URL: &str =
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";
    const MICROPHONE_SETTINGS_URL: &str =
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone";
    const SCREEN_RECORDING_SETTINGS_URL: &str =
        "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";

    #[derive(Clone, Copy)]
    pub struct NativePermissionProvider;

    fn microphone_status() -> MicrophoneState {
        use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

        let Some(media_type) = (unsafe { AVMediaTypeAudio }) else {
            return MicrophoneState::Restricted;
        };
        match unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type) } {
            AVAuthorizationStatus::NotDetermined => MicrophoneState::NotDetermined,
            AVAuthorizationStatus::Denied => MicrophoneState::Denied,
            AVAuthorizationStatus::Restricted => MicrophoneState::Restricted,
            AVAuthorizationStatus::Authorized => MicrophoneState::Granted,
            _ => MicrophoneState::Restricted,
        }
    }

    fn open_privacy_settings(url: &str, label: &str) -> Result<(), String> {
        let status = std::process::Command::new("open")
            .arg(url)
            .status()
            .map_err(|error| format!("open failed: {error}"))?;
        if !status.success() {
            return Err(format!("System Settings exited with {status}"));
        }
        eprintln!("[onboarding] opened System Settings > {label}");
        Ok(())
    }

    impl PermissionProvider for NativePermissionProvider {
        fn status(&self) -> RawPermissionStatus {
            RawPermissionStatus {
                accessibility: axcache::ax_trusted_silent(),
                microphone: microphone_status(),
                screen_recording_effective: objc2_core_graphics::CGPreflightScreenCaptureAccess(),
            }
        }

        fn request_accessibility(&self) -> Result<(), String> {
            let _ = axcache::ax_trusted();
            open_privacy_settings(AX_SETTINGS_URL, "Accessibility")
        }

        fn request_microphone(&self, finished: Box<dyn FnOnce() + Send>) -> Result<(), String> {
            use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

            let media_type = (unsafe { AVMediaTypeAudio })
                .ok_or_else(|| "AVFoundation audio media type unavailable".to_owned())?;
            match unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type) } {
                AVAuthorizationStatus::Authorized => {
                    finished();
                    Ok(())
                }
                AVAuthorizationStatus::NotDetermined => {
                    let finished = std::sync::Mutex::new(Some(finished));
                    let completion = block2::RcBlock::new(move |_granted: objc2::runtime::Bool| {
                        if let Some(finished) = finished
                            .lock()
                            .ok()
                            .and_then(|mut callback| callback.take())
                        {
                            finished();
                        }
                    });
                    unsafe {
                        AVCaptureDevice::requestAccessForMediaType_completionHandler(
                            media_type,
                            &completion,
                        );
                    }
                    Ok(())
                }
                AVAuthorizationStatus::Denied | AVAuthorizationStatus::Restricted => {
                    open_privacy_settings(MICROPHONE_SETTINGS_URL, "Microphone")?;
                    finished();
                    Ok(())
                }
                _ => Err("unknown microphone authorization state".to_owned()),
            }
        }

        fn request_screen_recording(&self) -> Result<ScreenRequestResult, String> {
            use objc2_core_graphics::{
                CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess,
            };

            if CGPreflightScreenCaptureAccess() {
                return Ok(ScreenRequestResult::AlreadyEffective);
            }
            let granted = CGRequestScreenCaptureAccess();
            if !granted {
                open_privacy_settings(SCREEN_RECORDING_SETTINGS_URL, "Screen Recording")?;
            }
            Ok(if granted {
                ScreenRequestResult::PromptGranted
            } else {
                ScreenRequestResult::SettingsOpened
            })
        }
    }

    pub struct PermissionRuntime(pub Mutex<PermissionCoordinator<NativePermissionProvider>>);

    impl Default for PermissionRuntime {
        fn default() -> Self {
            Self(Mutex::new(PermissionCoordinator::new(
                NativePermissionProvider,
            )))
        }
    }

    /// Call while holding `PermissionRuntime`'s coordinator mutex. Revision assignment and event
    /// submission must share one serialized section or a delayed producer can emit N after N+1.
    fn emit_edge(app: &AppHandle, edge: Option<PermissionSnapshot>) {
        if let Some(snapshot) = edge {
            let _ = app.emit("permissions-changed", snapshot);
        }
    }

    pub fn status(app: &AppHandle) -> PermissionSnapshot {
        let Some(runtime) = app.try_state::<PermissionRuntime>() else {
            let mut coordinator = PermissionCoordinator::new(NativePermissionProvider);
            return coordinator.status().0;
        };
        let Ok(mut coordinator) = runtime.0.lock() else {
            return PermissionSnapshot::default();
        };
        coordinator.status().0
    }

    pub fn start(app: AppHandle) -> Option<u64> {
        let runtime = app.try_state::<PermissionRuntime>()?;
        let Ok(mut coordinator) = runtime.0.lock() else {
            return None;
        };
        let (generation, started) = coordinator.start();
        drop(coordinator);
        if !started {
            return Some(generation);
        }
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(500));
            let Some(runtime) = app.try_state::<PermissionRuntime>() else {
                return;
            };
            let Ok(mut coordinator) = runtime.0.lock() else {
                return;
            };
            if !coordinator.generation_active(generation) {
                return;
            }
            let edge = coordinator.poll(generation);
            let emit = coordinator.events_enabled();
            if emit {
                emit_edge(&app, edge);
            }
        });
        Some(generation)
    }

    pub fn stop(app: &AppHandle, generation: u64) {
        let Some(runtime) = app.try_state::<PermissionRuntime>() else {
            return;
        };
        if let Ok(mut coordinator) = runtime.0.lock() {
            coordinator.stop(generation);
        };
    }

    pub fn listener_ready(app: &AppHandle) -> PermissionSnapshot {
        let Some(runtime) = app.try_state::<PermissionRuntime>() else {
            return PermissionSnapshot::default();
        };
        let Ok(mut coordinator) = runtime.0.lock() else {
            return PermissionSnapshot::default();
        };
        let latest = coordinator.status().0;
        let initial = coordinator.listener_ready();
        emit_edge(app, initial);
        latest
    }

    fn request_finished(app: &AppHandle, screen_request: Option<ScreenRequestResult>) {
        let Some(runtime) = app.try_state::<PermissionRuntime>() else {
            return;
        };
        let Ok(mut coordinator) = runtime.0.lock() else {
            return;
        };
        let edge = coordinator.request_finished(screen_request);
        let emit = coordinator.events_enabled();
        if emit {
            emit_edge(app, edge);
        }
    }

    pub fn request_accessibility(app: &AppHandle) -> Result<(), String> {
        let runtime = app
            .try_state::<PermissionRuntime>()
            .ok_or_else(|| "permission coordinator unavailable".to_owned())?;
        let provider = *runtime
            .0
            .lock()
            .map_err(|_| "permission coordinator unavailable".to_owned())?
            .provider();
        provider.request_accessibility()?;
        request_finished(app, None);
        Ok(())
    }

    pub fn request_microphone(app: &AppHandle) -> Result<(), String> {
        let runtime = app
            .try_state::<PermissionRuntime>()
            .ok_or_else(|| "permission coordinator unavailable".to_owned())?;
        let callback_app = app.clone();
        let provider = *runtime
            .0
            .lock()
            .map_err(|_| "permission coordinator unavailable".to_owned())?
            .provider();
        provider.request_microphone(Box::new(move || request_finished(&callback_app, None)))
    }

    pub fn request_screen_recording(app: &AppHandle) -> Result<(), String> {
        let runtime = app
            .try_state::<PermissionRuntime>()
            .ok_or_else(|| "permission coordinator unavailable".to_owned())?;
        let provider = *runtime
            .0
            .lock()
            .map_err(|_| "permission coordinator unavailable".to_owned())?
            .provider();
        let result = provider.request_screen_recording()?;
        request_finished(app, Some(result));
        Ok(())
    }

    pub fn install_activation_observer(app: &tauri::App) {
        static INSTALLED: AtomicBool = AtomicBool::new(false);
        if INSTALLED.swap(true, Ordering::SeqCst) {
            return;
        }
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};
        use objc2_foundation::NSString;

        let handle = app.handle().clone();
        unsafe {
            let center: *mut AnyObject = msg_send![class!(NSNotificationCenter), defaultCenter];
            if center.is_null() {
                INSTALLED.store(false, Ordering::SeqCst);
                return;
            }
            let name = NSString::from_str("NSApplicationDidBecomeActiveNotification");
            let block = block2::RcBlock::new(move |_notification: *mut AnyObject| {
                let Some(runtime) = handle.try_state::<PermissionRuntime>() else {
                    return;
                };
                let Ok(mut coordinator) = runtime.0.lock() else {
                    return;
                };
                let edge = coordinator.activation_refresh();
                let emit = coordinator.events_enabled();
                if emit {
                    emit_edge(&handle, edge);
                }
            });
            let nil: *mut AnyObject = std::ptr::null_mut();
            let _observer: *mut AnyObject = msg_send![center, addObserverForName: &*name, object: nil, queue: nil, usingBlock: &*block];
            std::mem::forget(block);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use super::*;

    struct ProviderDouble {
        status: Mutex<RawPermissionStatus>,
        status_calls: AtomicUsize,
        request_calls: AtomicUsize,
    }

    impl ProviderDouble {
        fn new(status: RawPermissionStatus) -> Self {
            Self {
                status: Mutex::new(status),
                status_calls: AtomicUsize::new(0),
                request_calls: AtomicUsize::new(0),
            }
        }

        fn set(&self, status: RawPermissionStatus) {
            *self.status.lock().expect("status lock") = status;
        }
    }

    impl PermissionProvider for ProviderDouble {
        fn status(&self) -> RawPermissionStatus {
            self.status_calls.fetch_add(1, Ordering::Relaxed);
            *self.status.lock().expect("status lock")
        }

        fn request_accessibility(&self) -> Result<(), String> {
            self.request_calls.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        fn request_microphone(&self, finished: Box<dyn FnOnce() + Send>) -> Result<(), String> {
            self.request_calls.fetch_add(1, Ordering::Relaxed);
            finished();
            Ok(())
        }

        fn request_screen_recording(&self) -> Result<ScreenRequestResult, String> {
            self.request_calls.fetch_add(1, Ordering::Relaxed);
            Ok(ScreenRequestResult::PromptGranted)
        }
    }

    fn denied() -> RawPermissionStatus {
        RawPermissionStatus {
            accessibility: false,
            microphone: MicrophoneState::Denied,
            screen_recording_effective: false,
        }
    }

    #[test]
    fn status_path_never_invokes_request_methods() {
        let provider = ProviderDouble::new(denied());
        let mut coordinator = PermissionCoordinator::new(provider);
        let _ = coordinator.status();
        assert_eq!(
            coordinator.provider().request_calls.load(Ordering::Relaxed),
            0
        );
    }

    #[test]
    fn listener_ready_initial_edge_once_and_generation_cancellation() {
        let provider = ProviderDouble::new(denied());
        let mut coordinator = PermissionCoordinator::new(provider);
        let (generation, started) = coordinator.start();
        assert!(started);
        assert!(!coordinator.events_enabled());
        assert_eq!(coordinator.listener_ready().expect("initial").revision, 1);
        assert!(coordinator.events_enabled());
        assert!(coordinator.listener_ready().is_none());
        assert!(coordinator.poll(generation).is_none());

        coordinator.provider().set(RawPermissionStatus {
            accessibility: true,
            ..denied()
        });
        assert_eq!(coordinator.poll(generation).expect("edge").revision, 2);

        coordinator.provider().set(RawPermissionStatus {
            accessibility: true,
            microphone: MicrophoneState::Granted,
            screen_recording_effective: false,
        });
        assert_eq!(
            coordinator
                .request_finished(None)
                .expect("request edge")
                .revision,
            3
        );

        coordinator.provider().set(RawPermissionStatus {
            accessibility: true,
            microphone: MicrophoneState::Granted,
            screen_recording_effective: true,
        });
        assert_eq!(
            coordinator
                .activation_refresh()
                .expect("activation edge")
                .revision,
            4
        );

        coordinator.stop(generation);
        assert!(coordinator.events_enabled());
        coordinator.provider().set(denied());
        assert!(coordinator.poll(generation).is_none());
        assert_eq!(
            coordinator
                .activation_refresh()
                .expect("app lifetime activation edge")
                .revision,
            5
        );
    }

    #[test]
    fn granted_screen_request_is_restart_required_until_effective() {
        let provider = ProviderDouble::new(RawPermissionStatus {
            accessibility: true,
            microphone: MicrophoneState::Granted,
            screen_recording_effective: false,
        });
        let mut coordinator = PermissionCoordinator::new(provider);
        let _ = coordinator.status();
        let edge = coordinator
            .request_finished(Some(ScreenRequestResult::PromptGranted))
            .expect("restart edge");
        assert_eq!(
            edge.screen_recording_state,
            ScreenRecordingState::RestartRequired
        );
        assert!(!edge.all_effective);
    }

    #[test]
    fn settings_repair_reason_is_honest_and_later_revoke_is_not_restart_required() {
        let provider = ProviderDouble::new(RawPermissionStatus {
            accessibility: true,
            microphone: MicrophoneState::Granted,
            screen_recording_effective: false,
        });
        let mut coordinator = PermissionCoordinator::new(provider);
        let _ = coordinator.status();
        let repair = coordinator
            .request_finished(Some(ScreenRequestResult::SettingsOpened))
            .expect("settings repair edge");
        assert_eq!(
            repair.screen_recording_state,
            ScreenRecordingState::NotGranted
        );
        assert_eq!(
            repair.reason,
            Some(PermissionReason::ScreenRecordingSettingsRepairPending)
        );

        coordinator.provider().set(RawPermissionStatus {
            accessibility: true,
            microphone: MicrophoneState::Granted,
            screen_recording_effective: true,
        });
        assert_eq!(
            coordinator
                .activation_refresh()
                .expect("effective edge")
                .screen_recording_state,
            ScreenRecordingState::Granted
        );
        coordinator.provider().set(RawPermissionStatus {
            accessibility: true,
            microphone: MicrophoneState::Granted,
            screen_recording_effective: false,
        });
        let revoked = coordinator.activation_refresh().expect("revoke edge");
        assert_eq!(
            revoked.screen_recording_state,
            ScreenRecordingState::NotGranted
        );
        assert_eq!(revoked.reason, None);
    }
}
