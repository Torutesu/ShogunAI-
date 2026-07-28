//! System audio via a Core Audio process tap (`CATapDescription` / `AudioHardwareCreateProcessTap`,
//! macOS 14.4+). Speaker = `Other`. This is how the other side of a meeting is captured without a
//! bot joining the call (Issue #7 Non-Goal).
//!
//! On macOS 14.0–14.3 the tap API does not exist, so `open` returns `Ok(None)` and the lane runs
//! mic-only (§7). A TCC denial (or any Core Audio create failure) also returns `Ok(None)`. Never a
//! hard error — the meeting still records from the mic.
//!
//! Flow (mirrors `insidegui/AudioCap`): build a private, unmuted global `CATapDescription` that
//! excludes our own process → `AudioHardwareCreateProcessTap` → read the tap's stream format and
//! UID → create a private aggregate device whose tap-list is that UID →
//! `AudioDeviceCreateIOProcIDWithBlock` with a block that downmixes+resamples each input buffer to
//! 16 kHz mono f32 and sends it on an in-RAM channel (invariant 2: never a file/temp) →
//! `AudioDeviceStart`. Teardown reverses all of it on `stop`/`Drop`.

use super::super::Speaker;
use super::{AudioSource, Frame};
use std::sync::mpsc::{Receiver, TryRecvError};

pub struct SystemTap {
    rx: Receiver<Vec<f32>>,
    #[cfg(all(feature = "audio", target_os = "macos"))]
    running: Option<ffi::TapStream>,
}

impl SystemTap {
    /// `Ok(None)` = not available on this OS / permission (degrade to mic-only). `Ok(Some)` = tap
    /// running. `Err` is reserved for genuinely unexpected failures the caller logs once.
    pub fn open() -> Result<Option<Self>, String> {
        if !process_tap_supported() {
            eprintln!("[meeting] system audio tap unavailable (needs macOS 14.4+); mic only");
            return Ok(None);
        }
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
        // Idempotent: release the aggregate device + tap. `TapStream::Drop` does the real work; we
        // take it so a later `stop`/`Drop` is a no-op.
        #[cfg(all(feature = "audio", target_os = "macos"))]
        {
            self.running.take();
        }
    }
}

impl Drop for SystemTap {
    fn drop(&mut self) {
        self.stop();
    }
}

/// `true` when the running macOS is at least `major.minor`, via `NSProcessInfo`.
fn macos_at_least(major: isize, minor: isize) -> bool {
    use objc2_foundation::NSProcessInfo;
    let v = NSProcessInfo::processInfo().operatingSystemVersion();
    (v.majorVersion, v.minorVersion) >= (major, minor)
}

#[cfg(all(feature = "audio", target_os = "macos"))]
fn create_tap_stream() -> Result<Option<SystemTap>, String> {
    let (tx, rx) = std::sync::mpsc::channel::<Vec<f32>>();
    match ffi::start(tx) {
        Ok(Some(running)) => Ok(Some(SystemTap { rx, running: Some(running) })),
        // A create/permission failure degrades to mic-only (Ok(None)), matching the contract.
        Ok(None) => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(not(all(feature = "audio", target_os = "macos")))]
fn create_tap_stream() -> Result<Option<SystemTap>, String> {
    Ok(None)
}

// ---------------------------------------------------------------------------
// Core Audio process-tap FFI (macOS 14.4+). Everything here is `unsafe` FFI; each block documents
// the invariant it upholds. All bindings below come from `objc2-core-audio` 0.3.2 —
// `CATapDescription`, `AudioHardwareCreateProcessTap`/`DestroyProcessTap`,
// `AudioHardwareCreateAggregateDevice`/`DestroyAggregateDevice`,
// `AudioDeviceCreateIOProcIDWithBlock`, `AudioDeviceDestroyIOProcID`, `AudioDeviceStart`/`Stop`,
// `AudioObjectGetPropertyData` and the property/aggregate-key constants — plus `objc2-foundation`
// (NSArray/NSString/NSNumber/NSUUID/NSDictionary), `objc2-core-audio-types`
// (AudioStreamBasicDescription/AudioBufferList), `objc2-core-foundation` (CFDictionary), `block2`
// (the IOProc block) and `dispatch2` (its serial queue). No hand-rolled `extern "C"` was needed.
#[cfg(all(feature = "audio", target_os = "macos"))]
mod ffi {
    use crate::audio::resample;
    use block2::RcBlock;
    use dispatch2::DispatchQueue;
    use objc2::rc::Retained;
    use objc2::AnyThread;
    use objc2_core_audio::{
        kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceNameKey,
        kAudioAggregateDeviceTapListKey, kAudioAggregateDeviceUIDKey, kAudioObjectPropertyElementMain,
        kAudioObjectPropertyScopeGlobal, kAudioSubTapUIDKey, kAudioTapPropertyFormat,
        kAudioTapPropertyUID, AudioDeviceCreateIOProcIDWithBlock, AudioDeviceDestroyIOProcID,
        AudioDeviceIOProcID, AudioDeviceStart, AudioDeviceStop, AudioHardwareCreateAggregateDevice,
        AudioHardwareCreateProcessTap, AudioHardwareDestroyAggregateDevice,
        AudioHardwareDestroyProcessTap, AudioObjectGetPropertyData, AudioObjectID,
        AudioObjectPropertyAddress, CATapDescription, CATapMuteBehavior,
    };
    use objc2_core_audio_types::{AudioBufferList, AudioStreamBasicDescription};
    use objc2_core_foundation::CFDictionary;
    use objc2_foundation::{
        NSArray, NSDictionary, NSNumber, NSObject, NSString, NSUUID,
    };
    use std::ffi::c_void;
    use std::ptr::NonNull;
    use std::sync::mpsc::Sender;

    /// A live tap: the IDs needed to tear everything down, in the exact reverse order of creation.
    /// Holding the `RcBlock` keeps the IOProc's closure alive for as long as the proc is registered.
    pub struct TapStream {
        aggregate_id: AudioObjectID,
        tap_id: AudioObjectID,
        io_proc_id: AudioDeviceIOProcID,
        // Retained so the block backing `io_proc_id` outlives the running device. The concrete
        // signature matches what `AudioDeviceCreateIOProcIDWithBlock` expects.
        #[allow(clippy::type_complexity)]
        _block: RcBlock<
            dyn Fn(
                NonNull<objc2_core_audio_types::AudioTimeStamp>,
                NonNull<AudioBufferList>,
                NonNull<objc2_core_audio_types::AudioTimeStamp>,
                NonNull<AudioBufferList>,
                NonNull<objc2_core_audio_types::AudioTimeStamp>,
            ),
        >,
    }

    // The teardown only touches plain integer AudioObjectIDs + an owned RcBlock; it is safe to move
    // the handle across threads (the desktop keeps the source on the worker thread).
    unsafe impl Send for TapStream {}

    impl Drop for TapStream {
        fn drop(&mut self) {
            // Reverse of `start`: stop IO, remove the IOProc, destroy the aggregate device, then the
            // tap. Every call is best-effort — a failed teardown must not panic the capture path.
            unsafe {
                AudioDeviceStop(self.aggregate_id, self.io_proc_id);
                if self.io_proc_id.is_some() {
                    AudioDeviceDestroyIOProcID(self.aggregate_id, self.io_proc_id);
                }
                AudioHardwareDestroyAggregateDevice(self.aggregate_id);
                AudioHardwareDestroyProcessTap(self.tap_id);
            }
        }
    }

    fn addr(selector: u32) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress {
            mSelector: selector,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        }
    }

    /// Build the tap + aggregate device and start IO. `Ok(Some)` = running; `Ok(None)` = a Core
    /// Audio call failed (e.g. TCC denial) so the caller degrades to mic-only; `Err` is reserved for
    /// states that should be impossible.
    pub fn start(tx: Sender<Vec<f32>>) -> Result<Option<TapStream>, String> {
        // 1. Private, unmuted, global tap excluding our own process (empty exclude list == all
        //    system output). `CATapDescription` and its setters are objc2 msg-sends.
        let desc: Retained<CATapDescription> = unsafe {
            let empty: Retained<NSArray<NSNumber>> = NSArray::new();
            let d = CATapDescription::initStereoGlobalTapButExcludeProcesses(
                CATapDescription::alloc(),
                &empty,
            );
            d.setName(&NSString::from_str("SHOGUN meeting tap"));
            d.setUUID(&NSUUID::new());
            d.setPrivate(true);
            d.setMuteBehavior(CATapMuteBehavior::Unmuted);
            d
        };

        // 2. Create the tap object.
        let mut tap_id: AudioObjectID = 0;
        let status =
            unsafe { AudioHardwareCreateProcessTap(Some(&desc), &mut tap_id as *mut AudioObjectID) };
        if status != 0 || tap_id == 0 {
            // Most commonly a TCC (Audio Recording) denial. Degrade, don't error.
            eprintln!("[meeting] process tap create failed (status {status}); mic only");
            return Ok(None);
        }
        // From here on, any early return must destroy the tap so we never leak it.
        let tap_guard = TapGuard(tap_id);

        // 3a. Read the tap's stream format (sample rate + channel count for the resample).
        // SAFETY: an all-zero ASBD is a valid "empty" struct; `AudioObjectGetPropertyData` fills it.
        let mut asbd: AudioStreamBasicDescription = unsafe { std::mem::zeroed() };
        let mut size = std::mem::size_of::<AudioStreamBasicDescription>() as u32;
        let mut fmt_addr = addr(kAudioTapPropertyFormat);
        let status = unsafe {
            AudioObjectGetPropertyData(
                tap_id,
                NonNull::from(&mut fmt_addr),
                0,
                std::ptr::null(),
                NonNull::from(&mut size),
                NonNull::new(&mut asbd as *mut _ as *mut c_void).ok_or("null asbd ptr")?,
            )
        };
        if status != 0 {
            eprintln!("[meeting] tap format read failed (status {status}); mic only");
            return Ok(None);
        }
        let in_rate = if asbd.mSampleRate > 0.0 { asbd.mSampleRate as u32 } else { 48_000 };
        let channels = asbd.mChannelsPerFrame.max(1) as u16;

        // 3b. Read the tap's UID (a CFStringRef == toll-free NSString) for the aggregate tap-list.
        let mut tap_uid_ptr: *const NSString = std::ptr::null();
        let mut uid_size = std::mem::size_of::<*const NSString>() as u32;
        let mut uid_addr = addr(kAudioTapPropertyUID);
        let status = unsafe {
            AudioObjectGetPropertyData(
                tap_id,
                NonNull::from(&mut uid_addr),
                0,
                std::ptr::null(),
                NonNull::from(&mut uid_size),
                NonNull::new(&mut tap_uid_ptr as *mut _ as *mut c_void).ok_or("null uid ptr")?,
            )
        };
        if status != 0 || tap_uid_ptr.is_null() {
            eprintln!("[meeting] tap UID read failed (status {status}); mic only");
            return Ok(None);
        }
        // `AudioObjectGetPropertyData` for a CFString gives us a +1 reference we now own.
        let tap_uid: Retained<NSString> =
            unsafe { Retained::from_raw(tap_uid_ptr as *mut NSString) }
                .ok_or("tap UID retain failed")?;

        // 4. Private aggregate device whose only sub-tap is our tap UID.
        let aggregate_id = match build_aggregate_device(&tap_uid) {
            Ok(id) => id,
            Err(e) => {
                eprintln!("[meeting] aggregate device create failed ({e}); mic only");
                return Ok(None);
            }
        };
        let agg_guard = AggGuard(aggregate_id);

        // 5. IOProc block: downmix + resample each input buffer and send it on the RAM channel.
        //    Kept cheap — one resample and one `send`, no logging, no file I/O (invariant 2).
        let block = RcBlock::new(
            move |_now: NonNull<objc2_core_audio_types::AudioTimeStamp>,
                  input: NonNull<AudioBufferList>,
                  _in_time: NonNull<objc2_core_audio_types::AudioTimeStamp>,
                  _out: NonNull<AudioBufferList>,
                  _out_time: NonNull<objc2_core_audio_types::AudioTimeStamp>| {
                // SAFETY: the HAL hands us a valid, read-only AudioBufferList for this IO cycle.
                let list = unsafe { input.as_ref() };
                if list.mNumberBuffers == 0 {
                    return;
                }
                let buf = &list.mBuffers[0];
                if buf.mData.is_null() || buf.mDataByteSize == 0 {
                    return;
                }
                let n = buf.mDataByteSize as usize / std::mem::size_of::<f32>();
                // SAFETY: tap output is 32-bit float PCM; `n` is derived from the buffer's own size.
                let samples: &[f32] = unsafe { std::slice::from_raw_parts(buf.mData as *const f32, n) };
                // The buffer's own channel count is authoritative; fall back to the ASBD's if the
                // HAL reports zero.
                let ch = match buf.mNumberChannels {
                    0 => channels,
                    n => n as u16,
                };
                let mono = resample::to_mono(samples, ch);
                let frame = resample::to_16k_mono(&mono, in_rate);
                // Receiver gone (worker stopped) → capture is over; dropping the frame is correct.
                let _ = tx.send(frame);
            },
        );
        // The block type the FFI wants is `*mut DynBlock<dyn Fn(...)>`.
        let block_ptr = &*block as *const _ as *mut _;

        // Dedicated serial queue so the IOProc never contends with the mic's Core Audio thread.
        let queue = DispatchQueue::new("ai.shogun.meeting.tap", None);
        let mut io_proc_id: AudioDeviceIOProcID = None;
        let status = unsafe {
            AudioDeviceCreateIOProcIDWithBlock(
                NonNull::from(&mut io_proc_id),
                aggregate_id,
                Some(&queue),
                block_ptr,
            )
        };
        if status != 0 || io_proc_id.is_none() {
            eprintln!("[meeting] IOProc create failed (status {status}); mic only");
            return Ok(None); // guards below tear down the aggregate + tap
        }

        // 6. Start IO.
        let status = unsafe { AudioDeviceStart(aggregate_id, io_proc_id) };
        if status != 0 {
            eprintln!("[meeting] AudioDeviceStart failed (status {status}); mic only");
            unsafe { AudioDeviceDestroyIOProcID(aggregate_id, io_proc_id) };
            return Ok(None); // guards tear down the aggregate + tap
        }

        // Success: disarm the guards; `TapStream::Drop` now owns teardown.
        std::mem::forget(tap_guard);
        std::mem::forget(agg_guard);
        Ok(Some(TapStream { aggregate_id, tap_id, io_proc_id, _block: block }))
    }

    /// Build the private aggregate device dictionary and create it. The description is an
    /// `NSDictionary` (toll-free bridged to the `CFDictionary` the create call takes).
    fn build_aggregate_device(tap_uid: &NSString) -> Result<AudioObjectID, String> {
        let uuid = NSUUID::new().UUIDString();
        // Sub-tap entry: { uid: <tap UID> }.
        let sub_tap: Retained<NSDictionary<NSString, NSObject>> = {
            let key = key_string(kAudioSubTapUIDKey);
            let keys: [&NSString; 1] = [&key];
            let vals: [&NSObject; 1] = [tap_uid.as_ref()];
            NSDictionary::from_slices(&keys, &vals)
        };
        let tap_list: Retained<NSArray<NSDictionary<NSString, NSObject>>> =
            NSArray::from_slice(&[&*sub_tap]);

        let name = NSString::from_str("SHOGUN Meeting Aggregate");
        let uid = uuid;
        let is_private = NSNumber::new_i32(1);

        // Description: { name, uid, private: 1, taps: [ {uid: tapUID} ] }.
        let desc: Retained<NSDictionary<NSString, NSObject>> = {
            let k_name = key_string(kAudioAggregateDeviceNameKey);
            let k_uid = key_string(kAudioAggregateDeviceUIDKey);
            let k_private = key_string(kAudioAggregateDeviceIsPrivateKey);
            let k_taps = key_string(kAudioAggregateDeviceTapListKey);
            let keys: [&NSString; 4] = [&k_name, &k_uid, &k_private, &k_taps];
            let vals: [&NSObject; 4] = [
                name.as_ref(),
                uid.as_ref(),
                is_private.as_ref(),
                tap_list.as_ref(),
            ];
            NSDictionary::from_slices(&keys, &vals)
        };

        // NSDictionary is toll-free bridged with CFDictionary; the pointer cast is sound.
        let cf: *const CFDictionary = Retained::as_ptr(&desc).cast();
        let mut aggregate_id: AudioObjectID = 0;
        let status = unsafe {
            AudioHardwareCreateAggregateDevice(
                // SAFETY: `cf` is a live, toll-free-bridged CFDictionary for the duration of the call.
                cf.as_ref().ok_or("null aggregate description")?,
                NonNull::from(&mut aggregate_id),
            )
        };
        if status != 0 || aggregate_id == 0 {
            return Err(format!("status {status}"));
        }
        Ok(aggregate_id)
    }

    /// The aggregate-device keys are `&CStr` (UTF-8, no interior NUL). Build an `NSString` from them.
    fn key_string(key: &std::ffi::CStr) -> Retained<NSString> {
        NSString::from_str(key.to_str().unwrap_or_default())
    }

    /// RAII: destroy the tap if we bail before wiring the full stream.
    struct TapGuard(AudioObjectID);
    impl Drop for TapGuard {
        fn drop(&mut self) {
            unsafe { AudioHardwareDestroyProcessTap(self.0) };
        }
    }

    /// RAII: destroy the aggregate device if we bail before wiring the full stream.
    struct AggGuard(AudioObjectID);
    impl Drop for AggGuard {
        fn drop(&mut self) {
            unsafe { AudioHardwareDestroyAggregateDevice(self.0) };
        }
    }
}
