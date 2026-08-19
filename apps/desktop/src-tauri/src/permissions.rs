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
    fn request_screen_recording(&self) -> Result<bool, String>;
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
    generation: u64,
    screen_restart_required: bool,
}

impl<P: PermissionProvider> PermissionCoordinator<P> {
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            latest: PermissionSnapshot::default(),
            initialized: false,
            active: false,
            generation: 0,
            screen_restart_required: false,
        }
    }

    fn sampled(&self) -> PermissionSnapshot {
        let raw = self.provider.status();
        let accessibility_state = if raw.accessibility {
            AccessibilityState::Granted
        } else {
            AccessibilityState::NotGranted
        };
        let screen_recording_state = if raw.screen_recording_effective {
            ScreenRecordingState::Granted
        } else if self.screen_restart_required {
            ScreenRecordingState::RestartRequired
        } else {
            ScreenRecordingState::NotGranted
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
            reason: (screen_recording_state == ScreenRecordingState::RestartRequired)
                .then_some(PermissionReason::ScreenRecordingRestartRequired),
            revision: self.latest.revision,
        }
    }

    fn refresh(&mut self) -> Option<PermissionSnapshot> {
        let mut sampled = self.sampled();
        if sampled.screen_recording_effective() {
            self.screen_restart_required = false;
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

    pub fn start(&mut self) -> (u64, Option<PermissionSnapshot>) {
        if self.active {
            return (self.generation, None);
        }
        self.generation = self.generation.wrapping_add(1);
        self.active = true;
        let edge = self.refresh();
        (self.generation, edge.or(Some(self.latest)))
    }

    pub fn stop(&mut self, generation: u64) {
        if self.active && self.generation == generation {
            self.active = false;
            self.generation = self.generation.wrapping_add(1);
        }
    }

    pub fn poll(&mut self, generation: u64) -> Option<PermissionSnapshot> {
        (self.active && self.generation == generation)
            .then(|| self.refresh())
            .flatten()
    }

    pub fn activation_refresh(&mut self) -> Option<PermissionSnapshot> {
        self.active.then(|| self.refresh()).flatten()
    }

    pub fn request_finished(
        &mut self,
        screen_request_granted: Option<bool>,
    ) -> Option<PermissionSnapshot> {
        if screen_request_granted == Some(true) {
            self.screen_restart_required = true;
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
        RawPermissionStatus,
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

        fn request_screen_recording(&self) -> Result<bool, String> {
            use objc2_core_graphics::{
                CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess,
            };

            if CGPreflightScreenCaptureAccess() {
                return Ok(true);
            }
            let granted = CGRequestScreenCaptureAccess();
            if !granted {
                open_privacy_settings(SCREEN_RECORDING_SETTINGS_URL, "Screen Recording")?;
            }
            Ok(granted)
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
        let (snapshot, edge) = coordinator.status();
        drop(coordinator);
        emit_edge(app, edge);
        snapshot
    }

    pub fn start(app: AppHandle) {
        let Some(runtime) = app.try_state::<PermissionRuntime>() else {
            return;
        };
        let Ok(mut coordinator) = runtime.0.lock() else {
            return;
        };
        let (generation, initial) = coordinator.start();
        drop(coordinator);
        let Some(initial) = initial else {
            return;
        };
        let _ = app.emit("permissions-changed", initial);
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(500));
            let Some(runtime) = app.try_state::<PermissionRuntime>() else {
                return;
            };
            let Ok(mut coordinator) = runtime.0.lock() else {
                return;
            };
            if app
                .get_webview_window(crate::onboarding::mac::ONBOARDING_LABEL)
                .is_none()
            {
                coordinator.stop(generation);
                return;
            }
            let edge = coordinator.poll(generation);
            drop(coordinator);
            emit_edge(&app, edge);
        });
    }

    fn request_finished(app: &AppHandle, screen_request_granted: Option<bool>) {
        let Some(runtime) = app.try_state::<PermissionRuntime>() else {
            return;
        };
        let Ok(mut coordinator) = runtime.0.lock() else {
            return;
        };
        let edge = coordinator.request_finished(screen_request_granted);
        drop(coordinator);
        emit_edge(app, edge);
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
        let granted = provider.request_screen_recording()?;
        request_finished(app, Some(granted));
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
                drop(coordinator);
                emit_edge(&handle, edge);
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

        fn request_screen_recording(&self) -> Result<bool, String> {
            self.request_calls.fetch_add(1, Ordering::Relaxed);
            Ok(true)
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
    fn initial_edge_once_request_activation_and_generation_cancellation() {
        let provider = ProviderDouble::new(denied());
        let mut coordinator = PermissionCoordinator::new(provider);
        let (generation, initial) = coordinator.start();
        assert_eq!(initial.expect("initial").revision, 1);
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
        coordinator.provider().set(denied());
        assert!(coordinator.poll(generation).is_none());
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
            .request_finished(Some(true))
            .expect("restart edge");
        assert_eq!(
            edge.screen_recording_state,
            ScreenRecordingState::RestartRequired
        );
        assert!(!edge.all_effective);
    }
}
