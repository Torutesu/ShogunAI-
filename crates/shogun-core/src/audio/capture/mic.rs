//! Microphone capture via cpal. Speaker = `Me`. cpal's `Stream` is `!Send` on macOS (the CoreAudio
//! handle is bound to the thread that built it), while `AudioSource` must be `Send` so the worker
//! can own it across threads. So the stream lives on its own dedicated thread that builds it, plays
//! it, and parks until stopped; `Mic` itself holds only `Send` handles — the receiving end of the
//! sample channel and a stop flag. Resampled 16 kHz mono frames arrive over the channel; the worker
//! polls `try_recv`.

use super::super::{resample, Speaker};
use super::{AudioSource, Frame};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;

pub struct Mic {
    rx: Receiver<Vec<f32>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Mic {
    /// Open the default input device. Err (permission denied, no device, unsupported config) →
    /// caller degrades to notes-only (meeting.rs), never crashes. Blocks only until the capture
    /// thread has built and started the stream (or reported why it could not).
    pub fn open() -> Result<Self, String> {
        Self::open_with_device(None)
    }

    /// Open a named input device, or the current macOS default when no name is selected.
    /// A missing selected device is an error rather than a silent fallback: changing the input
    /// source without the user's knowledge is surprising and can capture the wrong microphone.
    pub fn open_with_device(selected_device: Option<&str>) -> Result<Self, String> {
        let selected_device = selected_device.map(str::to_owned);
        let (sample_tx, rx) = std::sync::mpsc::channel::<Vec<f32>>();
        // The capture thread reports the outcome of building the stream back here so `open` stays
        // fallible without moving the `!Send` stream across the boundary.
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);

        let thread = std::thread::spawn(move || {
            let stream = match build_stream(sample_tx, selected_device) {
                Ok(s) => {
                    let _ = ready_tx.send(Ok(()));
                    s
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            // Keep the (`!Send`) stream alive on this thread until stop is requested. Dropping it
            // here stops and releases the device.
            while !stop_thread.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            drop(stream);
        });

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Mic {
                rx,
                stop,
                thread: Some(thread),
            }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err("mic capture thread exited before reporting readiness".into()),
        }
    }
}

/// Return display names for selectable input devices. Names are persisted because CPAL does not
/// expose a stable cross-platform device identifier; an exact name match avoids guessing after a
/// device is unplugged or renamed.
///
/// CPAL does not expose a stable macOS device identifier. Duplicate names collapse in the picker;
/// resolving an ambiguous persisted name fails rather than silently capturing a different device.
pub fn input_device_names() -> Result<Vec<String>, String> {
    let host = cpal::default_host();
    let mut names = host
        .input_devices()
        .map_err(|error| format!("could not list microphone devices: {error}"))?
        .filter_map(|device| device.name().ok())
        .collect::<Vec<_>>();
    names.sort_unstable();
    names.dedup();
    Ok(names)
}

fn push_resampled<T>(
    data: &[T],
    channels: u16,
    input_rate: u32,
    sample_tx: &std::sync::mpsc::Sender<Vec<f32>>,
) where
    T: cpal::Sample,
    f32: cpal::FromSample<T>,
{
    let samples: Vec<f32> = data.iter().copied().map(cpal::Sample::to_sample).collect();
    let mono = resample::to_mono(&samples, channels);
    let f16k = resample::to_16k_mono(&mono, input_rate);
    let _ = sample_tx.send(f16k);
}

fn build_stream_for<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: u16,
    input_rate: u32,
    sample_tx: std::sync::mpsc::Sender<Vec<f32>>,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let err_fn = |e| eprintln!("[voice] mic stream error: {e}");
    device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                push_resampled(data, channels, input_rate, &sample_tx);
            },
            err_fn,
            None,
        )
        .map_err(|e| e.to_string())
}

/// Pick the uniquely named device whose name matches `selected`, or report it as unavailable or
/// ambiguous.
///
/// Split out of `open_input_device` so the branch that must never silently fall back to another
/// input is testable without real hardware: it is generic over the device type, so a test can
/// hand it plain strings.
fn find_named_device<D>(
    devices: impl Iterator<Item = (String, D)>,
    selected: &str,
) -> Result<D, String> {
    let mut matches = devices
        .filter(|(name, _)| name == selected)
        .map(|(_, device)| device)
        .collect::<Vec<_>>();
    match matches.len() {
        0 => Err(format!("selected microphone is unavailable: {selected}")),
        1 => Ok(matches.pop().expect("one matching microphone")),
        _ => Err(format!(
            "selected microphone name is ambiguous: {selected}; choose a different input"
        )),
    }
}

/// Resolve the input device to capture from: the named selection, or the current macOS default
/// when nothing is selected. A selected-but-missing device is an error rather than a fallback —
/// switching the input source without the user's knowledge can capture the wrong microphone.
fn open_input_device(
    host: &cpal::Host,
    selected_device: Option<&str>,
) -> Result<cpal::Device, String> {
    let Some(selected_device) = selected_device else {
        return host
            .default_input_device()
            .ok_or_else(|| "no input device".to_string());
    };

    let devices = host
        .input_devices()
        .map_err(|error| format!("could not list microphone devices: {error}"))?
        .filter_map(|device| device.name().ok().map(|name| (name, device)));
    find_named_device(devices, selected_device)
}

/// Build and start the selected input stream, sending resampled 16 kHz mono frames on `sample_tx`.
/// Must run on the thread that will keep the returned stream alive (`Stream` is `!Send`).
fn build_stream(
    sample_tx: std::sync::mpsc::Sender<Vec<f32>>,
    selected_device: Option<String>,
) -> Result<cpal::Stream, String> {
    let host = cpal::default_host();
    let device = open_input_device(&host, selected_device.as_deref())?;
    let supported = device.default_input_config().map_err(|e| e.to_string())?;
    let input_rate = supported.sample_rate().0;
    let channels = supported.channels();
    let config = supported.config();
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => {
            build_stream_for::<f32>(&device, &config, channels, input_rate, sample_tx)
        }
        cpal::SampleFormat::F64 => {
            build_stream_for::<f64>(&device, &config, channels, input_rate, sample_tx)
        }
        cpal::SampleFormat::I8 => {
            build_stream_for::<i8>(&device, &config, channels, input_rate, sample_tx)
        }
        cpal::SampleFormat::I16 => {
            build_stream_for::<i16>(&device, &config, channels, input_rate, sample_tx)
        }
        cpal::SampleFormat::I24 => {
            build_stream_for::<cpal::I24>(&device, &config, channels, input_rate, sample_tx)
        }
        cpal::SampleFormat::I32 => {
            build_stream_for::<i32>(&device, &config, channels, input_rate, sample_tx)
        }
        cpal::SampleFormat::I64 => {
            build_stream_for::<i64>(&device, &config, channels, input_rate, sample_tx)
        }
        cpal::SampleFormat::U8 => {
            build_stream_for::<u8>(&device, &config, channels, input_rate, sample_tx)
        }
        cpal::SampleFormat::U16 => {
            build_stream_for::<u16>(&device, &config, channels, input_rate, sample_tx)
        }
        cpal::SampleFormat::U32 => {
            build_stream_for::<u32>(&device, &config, channels, input_rate, sample_tx)
        }
        cpal::SampleFormat::U64 => {
            build_stream_for::<u64>(&device, &config, channels, input_rate, sample_tx)
        }
        other => return Err(format!("unsupported microphone sample format: {other}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok(stream)
}

impl AudioSource for Mic {
    fn try_recv(&mut self) -> Option<Frame> {
        match self.rx.try_recv() {
            Ok(samples) => Some(Frame {
                speaker: Speaker::Me,
                samples,
            }),
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => None,
        }
    }
    fn stop(&mut self) {
        // Idempotent: signal the capture thread to drop the stream and release the device.
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Mic {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_resampled_downmixes_and_resamples_i16_input() {
        let (tx, rx) = std::sync::mpsc::channel();
        let stereo = [16_384_i16, 16_384, 0, 0];

        push_resampled(&stereo, 2, 32_000, &tx);

        let samples = rx.recv().expect("resampled samples should be sent");
        assert_eq!(samples.len(), 1);
        assert!((samples[0] - 0.5).abs() < 0.01);
    }

    fn devices(names: &[&str]) -> Vec<(String, String)> {
        names.iter().map(|n| ((*n).into(), (*n).into())).collect()
    }

    #[test]
    fn missing_selected_device_is_an_error_not_a_fallback() {
        let error = find_named_device(devices(&["Built-in Microphone"]).into_iter(), "Studio Mic")
            .expect_err("an unplugged selection must not resolve to another input");
        assert!(error.contains("Studio Mic"), "error names the device: {error}");
    }

    #[test]
    fn selected_device_resolves_by_exact_name() {
        let picked = find_named_device(
            devices(&["Built-in Microphone", "Studio Mic"]).into_iter(),
            "Studio Mic",
        )
        .expect("a connected selection resolves");
        assert_eq!(picked, "Studio Mic");
    }

    #[test]
    fn duplicate_names_are_rejected_instead_of_selecting_arbitrarily() {
        let candidates = vec![
            ("Studio Mic".to_string(), "first"),
            ("Studio Mic".to_string(), "second"),
        ];
        let error = find_named_device(candidates.into_iter(), "Studio Mic")
            .expect_err("an ambiguous device name must not select an arbitrary microphone");
        assert!(error.contains("ambiguous"), "error explains the ambiguity: {error}");
    }
}
