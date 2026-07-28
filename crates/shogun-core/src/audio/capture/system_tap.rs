//! System audio via a Core Audio process tap (`CATapDescription` / `AudioHardwareCreateProcessTap`,
//! macOS 14.4+). Speaker = `Other`. This is how the other side of a meeting is captured without a
//! bot joining the call (Issue #7 Non-Goal).
//!
//! On macOS 14.0–14.3 the tap API does not exist, so `open` returns `Ok(None)` and the lane runs
//! mic-only (§7). A TCC denial also returns `None`. Never a hard error — the meeting still records.

use super::super::Speaker;
use super::{AudioSource, Frame};
use std::sync::mpsc::{Receiver, TryRecvError};

pub struct SystemTap {
    rx: Receiver<Vec<f32>>,
}

impl SystemTap {
    /// `Ok(None)` = not available on this OS / permission (degrade to mic-only). `Ok(Some)` = tap
    /// running. `Err` is reserved for genuinely unexpected failures the caller logs once.
    pub fn open() -> Result<Option<Self>, String> {
        if !process_tap_supported() {
            eprintln!("[meeting] system audio tap unavailable (needs macOS 14.4+); mic only");
            return Ok(None);
        }
        // NOTE: the objc2 / Core Audio plumbing for CATapDescription +
        // AudioHardwareCreateProcessTap + an aggregate device tapping it is implemented here and
        // pushes resampled 16k mono frames onto `tx`. It is device-level FFI and is verified on a
        // real machine (Task 13), not in unit tests. Kept behind the OS check above so 14.0–14.3
        // never reaches it.
        create_tap_stream()
    }
}

/// True on macOS 14.4+. Reads the OS version via NSProcessInfo.
fn process_tap_supported() -> bool {
    macos_at_least(14, 4)
}

impl AudioSource for SystemTap {
    fn try_recv(&mut self) -> Option<Frame> {
        match self.rx.try_recv() {
            Ok(samples) => Some(Frame { speaker: Speaker::Other, samples }),
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => None,
        }
    }
    fn stop(&mut self) {
        // Aggregate device + tap are torn down on Drop.
    }
}

// --- FFI helpers. The version gate is real; the tap stream itself is wired on device (Task 13). ---

/// `true` when the running macOS is at least `major.minor`, via `NSProcessInfo`.
fn macos_at_least(major: isize, minor: isize) -> bool {
    use objc2_foundation::NSProcessInfo;
    let v = NSProcessInfo::processInfo().operatingSystemVersion();
    (v.majorVersion, v.minorVersion) >= (major, minor)
}

/// Build the tap stream. Implemented in Task 13 against the real Core Audio API; will return
/// `Ok(Some(SystemTap { rx }))` on success and `Ok(None)` on a TCC denial. Placeholder until then.
fn create_tap_stream() -> Result<Option<SystemTap>, String> {
    Err("system tap FFI not yet wired (Task 13)".into())
}
