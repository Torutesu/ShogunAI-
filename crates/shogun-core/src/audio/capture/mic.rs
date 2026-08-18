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
        let (sample_tx, rx) = std::sync::mpsc::channel::<Vec<f32>>();
        // The capture thread reports the outcome of building the stream back here so `open` stays
        // fallible without moving the `!Send` stream across the boundary.
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);

        let thread = std::thread::spawn(move || {
            let stream = match build_stream(sample_tx) {
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

/// Build and start the default input stream, sending resampled 16 kHz mono frames on `sample_tx`.
/// Must run on the thread that will keep the returned stream alive (`Stream` is `!Send`).
fn build_stream(sample_tx: std::sync::mpsc::Sender<Vec<f32>>) -> Result<cpal::Stream, String> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or("no input device")?;
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
}
