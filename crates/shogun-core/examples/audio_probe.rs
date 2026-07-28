//! Device probe for the MT3 audio lane (Issue #7) — NOT shipped code, a hand-run verification tool.
//!
//! Exercises the real capture → VAD → ASR path end to end without the app or the encrypted DB:
//! opens the microphone (speaker = me) and, on macOS 14.4+, the Core Audio system tap
//! (speaker = other), runs the same `Worker` the app uses, and prints each transcribed line to
//! stdout for a fixed duration, then tears everything down.
//!
//! It is the fastest way to confirm on a real machine that:
//!   - the TCC prompts appear (microphone, and audio-recording for the tap),
//!   - your own voice comes back as `me:` and other apps' audio (play a video) as `other:`,
//!   - whisper actually transcribes on this hardware,
//!   - stop/Drop releases the tap + aggregate device without crashing
//!     (the one tap-UID ref-count assumption to sanity-check on device).
//!
//! Run (needs a real ggml/gguf whisper model):
//!   SHOGUN_WHISPER_MODEL=$PWD/models/whisper/ggml-small.bin \
//!     cargo run -p shogun-core --features audio --release --example audio_probe -- 25
//!   (the trailing number is seconds to listen; default 20)
//!
//! Invariant 2: audio stays in RAM; this tool writes text to the console, never a file.

#[cfg(all(feature = "audio", target_os = "macos"))]
fn main() {
    use shogun_core::audio::asr::whisper::Whisper;
    use shogun_core::audio::capture::mic::Mic;
    use shogun_core::audio::capture::system_tap::SystemTap;
    use shogun_core::audio::capture::{AudioSource, MultiSource};
    use shogun_core::audio::worker::{SegmentSink, Worker};
    use shogun_core::audio::{Speaker, Utterance};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    fn now_ms() -> i64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
    }

    let secs: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(20);

    let model = match std::env::var("SHOGUN_WHISPER_MODEL") {
        Ok(m) => m,
        Err(_) => {
            eprintln!("set SHOGUN_WHISPER_MODEL to a ggml/gguf whisper model path");
            std::process::exit(2);
        }
    };
    // English-base, to mirror the shipped default (MeetingLanguage::English, §8): the probe should
    // reflect how the app actually transcribes, not whisper's raw per-utterance auto-detect. Pass
    // Some("ja") here if you are probing a Japanese meeting, or None to exercise the Auto path.
    let asr = match Whisper::load_with_language(&model, Some("en")) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("whisper load failed: {e}");
            std::process::exit(1);
        }
    };

    let mic = match Mic::open() {
        Ok(m) => {
            println!("mic: open (speaker = me)");
            m
        }
        Err(e) => {
            eprintln!("mic unavailable: {e}");
            std::process::exit(1);
        }
    };

    let mut sources: Vec<Box<dyn AudioSource>> = vec![Box::new(mic)];
    match SystemTap::open() {
        Ok(Some(tap)) => {
            println!("system tap: open (speaker = other) — play a video to test");
            sources.push(Box::new(tap));
        }
        Ok(None) => println!("system tap: unavailable (macOS < 14.4 or denied) — mic only"),
        Err(e) => println!("system tap: error ({e}) — mic only"),
    }

    struct StdoutSink;
    impl SegmentSink for StdoutSink {
        fn emit(&mut self, u: &Utterance, text: &str, confidence: f64) {
            let who = match u.speaker {
                Speaker::Me => "me",
                Speaker::Other => "other",
            };
            println!("[{who}] ({confidence:.2}) {text}");
        }
    }

    let mut worker = Worker::new(MultiSource::new(sources), asr);
    let mut sink = StdoutSink;
    println!("listening for {secs}s — speak, and play some audio…\n");

    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        if worker.poll(now_ms(), &mut sink) == 0 {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    println!("\nstopping (flushing final utterances, releasing devices)…");
    worker.stop(now_ms(), &mut sink);
    println!("done — if we got here without a crash, teardown is clean.");
}

#[cfg(not(all(feature = "audio", target_os = "macos")))]
fn main() {
    eprintln!("build with --features audio on macOS: cargo run -p shogun-core --features audio --example audio_probe");
}
