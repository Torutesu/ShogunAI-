//! "Is anyone using the microphone right now?" — the meeting signal that does not depend on
//! knowing which app or which URL (FR-MT-04 signal ②).
//!
//! A URL says a Meet page is open, which is equally true in the lobby, during the call, and
//! after everyone has left. A bundle-id table only knows the apps someone remembered to list.
//! Whether the input device is *running* is the thing that actually distinguishes a meeting from
//! a tab: it is true exactly while people are talking through the machine, in any app.
//!
//! **This reads a boolean and nothing else.** `kAudioDevicePropertyDeviceIsRunningSomewhere` is
//! a truth value about the device, not a sample buffer — no audio is captured, no microphone
//! permission is requested, and none is required to ask (FR-MT-12). The boundary between
//! detecting a meeting and listening to one is exactly this line, so nothing in this file opens
//! a stream.
//!
//! # What this cannot answer, and who compensates
//!
//! The property says *a* process is using *a* device. It does not say which process — so a voice
//! utility that holds an input from login makes it permanently true, and read naively that is a
//! meeting in Finder, in Slack and in the login window alike (observed on-device 2026-07-31).
//! [`shogun_core::meeting::detect::MicWatch`] compensates behaviourally: fed
//! [`MicSource::SystemWide`](shogun_core::meeting::detect::MicSource::SystemWide) plus the
//! frontmost app, it writes the signal off once the stretch has outlived three unrelated apps.
//!
//! The real fix is attribution, and it belongs here: on macOS 14.4+, CoreAudio exposes process
//! objects (`kAudioHardwarePropertyProcessObjectList`, then each object's
//! `kAudioProcessPropertyIsRunningInput` and `kAudioProcessPropertyPID`). Resolving the PID to a
//! bundle id and reporting
//! [`MicSource::Holder`](shogun_core::meeting::detect::MicSource::Holder) lets the watch answer
//! *who* is talking instead of guessing — and the detector already handles it. Keep the same
//! discipline when adding it: process objects expose running state, never samples.

#[cfg(target_os = "macos")]
pub use mac::input_in_use;

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::c_void;

    type AudioObjectID = u32;
    type OSStatus = i32;

    #[repr(C)]
    struct AudioObjectPropertyAddress {
        selector: u32,
        scope: u32,
        element: u32,
    }

    /// Four-character codes, as CoreAudio spells them.
    const fn fourcc(s: &[u8; 4]) -> u32 {
        ((s[0] as u32) << 24) | ((s[1] as u32) << 16) | ((s[2] as u32) << 8) | (s[3] as u32)
    }

    const SYSTEM_OBJECT: AudioObjectID = 1;
    const DEFAULT_INPUT_DEVICE: u32 = fourcc(b"dIn ");
    const DEVICES: u32 = fourcc(b"dev#");
    const IS_RUNNING_SOMEWHERE: u32 = fourcc(b"gone");
    const STREAM_CONFIGURATION: u32 = fourcc(b"slay");
    const SCOPE_GLOBAL: u32 = fourcc(b"glob");
    const SCOPE_INPUT: u32 = fourcc(b"inpt");
    const ELEMENT_MAIN: u32 = 0;
    const NO_ERROR: OSStatus = 0;

    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        fn AudioObjectGetPropertyDataSize(
            object: AudioObjectID,
            address: *const AudioObjectPropertyAddress,
            qualifier_size: u32,
            qualifier: *const c_void,
            data_size: *mut u32,
        ) -> OSStatus;
        fn AudioObjectGetPropertyData(
            object: AudioObjectID,
            address: *const AudioObjectPropertyAddress,
            qualifier_size: u32,
            qualifier: *const c_void,
            data_size: *mut u32,
            data: *mut c_void,
        ) -> OSStatus;
    }

    fn address(selector: u32, scope: u32) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress {
            selector,
            scope,
            element: ELEMENT_MAIN,
        }
    }

    /// Read a `u32`-sized property, or `None` if CoreAudio declines to answer.
    fn read_u32(object: AudioObjectID, selector: u32) -> Option<u32> {
        let addr = address(selector, SCOPE_GLOBAL);
        let mut value: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: `addr` and `value` are live, correctly sized locals; CoreAudio writes at most
        // `size` bytes into `value`. No ownership is transferred either way.
        let status = unsafe {
            AudioObjectGetPropertyData(
                object,
                &addr,
                0,
                std::ptr::null(),
                &mut size,
                (&mut value as *mut u32).cast::<c_void>(),
            )
        };
        (status == NO_ERROR).then_some(value)
    }

    /// Every audio object the system knows about.
    fn all_devices() -> Vec<AudioObjectID> {
        let addr = address(DEVICES, SCOPE_GLOBAL);
        let mut size: u32 = 0;
        // SAFETY: `addr` is a live local; the call only writes the byte count into `size`.
        let status = unsafe {
            AudioObjectGetPropertyDataSize(SYSTEM_OBJECT, &addr, 0, std::ptr::null(), &mut size)
        };
        if status != NO_ERROR || size == 0 {
            return Vec::new();
        }
        let count = size as usize / std::mem::size_of::<AudioObjectID>();
        let mut ids = vec![0 as AudioObjectID; count];
        // SAFETY: `ids` has room for exactly `size` bytes, which is what CoreAudio was asked for.
        let status = unsafe {
            AudioObjectGetPropertyData(
                SYSTEM_OBJECT,
                &addr,
                0,
                std::ptr::null(),
                &mut size,
                ids.as_mut_ptr().cast::<c_void>(),
            )
        };
        if status == NO_ERROR {
            ids
        } else {
            Vec::new()
        }
    }

    /// Whether a device has any input channels — i.e. whether it can be a microphone at all.
    fn has_input(device: AudioObjectID) -> bool {
        let addr = address(STREAM_CONFIGURATION, SCOPE_INPUT);
        let mut size: u32 = 0;
        // SAFETY: as above — size query only.
        let status = unsafe {
            AudioObjectGetPropertyDataSize(device, &addr, 0, std::ptr::null(), &mut size)
        };
        // An AudioBufferList with no buffers is just its header; anything larger means channels.
        status == NO_ERROR && size as usize > std::mem::size_of::<u32>()
    }

    /// Whether **any** input device is currently in use by any process.
    ///
    /// Every input device, not just the default one: a headset, a capture device or a virtual
    /// input can carry the call while the default stays idle, and checking only the default is
    /// how a meeting in progress reads as silence.
    ///
    /// Failure answers `false`. A machine with no input, or a CoreAudio that will not say, must
    /// not be read as "a meeting is happening" — a missed meeting costs one tap to start by
    /// hand, a false one puts a panel on screen while the user is doing something else.
    pub fn input_in_use() -> bool {
        // The default input first: it answers for the common case without enumerating anything.
        if let Some(device) = read_u32(SYSTEM_OBJECT, DEFAULT_INPUT_DEVICE) {
            if device != 0
                && read_u32(device, IS_RUNNING_SOMEWHERE).is_some_and(|running| running != 0)
            {
                return true;
            }
        }
        all_devices()
            .into_iter()
            .filter(|d| has_input(*d))
            .any(|d| read_u32(d, IS_RUNNING_SOMEWHERE).is_some_and(|running| running != 0))
    }
}

#[cfg(not(target_os = "macos"))]
pub fn input_in_use() -> bool {
    false
}
