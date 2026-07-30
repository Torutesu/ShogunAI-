//! push-to-talk の一発ASRレーン（Issue #44）。
//!
//! [`crate::audio_lane`] との違いは3つあり、どれも意図的:
//!
//! 1. **マイクだけを開く。** system tap を開かない。押して話しているのは目の前の一人で、
//!    その人の質問に答えるのが仕事だから、部屋の他の音を拾う理由がない。
//! 2. **DBに書かない。** 1回の発話は1回のプロンプトになって消える。`sessions` に対応する
//!    interval も無い。
//! 3. **劣化しない。** 会議はマイクが死んでもノートは録れるので notes-only に落ちるが、
//!    こちらは音声が全てなので、始められないなら理由を出して止まる。黙って無反応になるのが
//!    最悪の結果。
//!
//! 不変条件2は [`crate::audio_lane`] と同じ理屈で守られる: 波形は `Worker` のバッファにしか
//! 存在せず、ここが受け取るのは文字起こし後のテキストだけ。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use shogun_core::audio::worker::{SegmentSink, Worker};
use shogun_core::audio::Utterance;
use shogun_core::meeting::settings::{AsrModel, MeetingLanguage};
use shogun_core::ptt::buffer_sink::BufferSink;
use shogun_core::ptt::statemachine::Fail;

/// 動作中の一発レーン。`stop` か `discard` で必ず回収する。
pub struct Handle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    /// ポーリングスレッドと停止側の両方から触るので `Mutex`。中身はテキストだけ。
    sink: Arc<Mutex<BufferSink>>,
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// `Mutex` 越しに `SegmentSink` を満たすためのアダプタ。`Worker::poll` は
/// `&mut dyn SegmentSink` を取るので、ロックを取った状態のものを渡す。
struct Locked<'a>(std::sync::MutexGuard<'a, BufferSink>);

impl SegmentSink for Locked<'_> {
    fn emit(&mut self, u: &Utterance, text: &str, confidence: f64) {
        self.0.emit(u, text, confidence);
    }
}

/// マイクを開いて文字起こしを始める。
///
/// 失敗は [`Fail`] で返す — 呼び出し側はそれをそのまま状態機械の `Input::Failed` に渡せる。
pub fn start(
    app: &tauri::AppHandle,
    model: AsrModel,
    language: MeetingLanguage,
) -> Result<Handle, Fail> {
    let Some(model_path) = crate::audio_lane::select_model_path(app, model) else {
        eprintln!("[ptt] no whisper model available");
        return Err(Fail::NoAsrModel);
    };
    let asr = shogun_core::audio::asr::whisper::Whisper::load_with_language(
        &model_path.to_string_lossy(),
        language.whisper_code(),
    )
    .map_err(|e| {
        eprintln!("[ptt] whisper load failed ({e})");
        Fail::NoAsrModel
    })?;

    // マイクだけを開く。`Mic` は `AudioSource` を実装しているので、`Worker` にそのまま渡せる。
    // 会議レーンの `MultiSource` はソースが複数あるときのラウンドロビン用で、1本しか無い
    // ここでは剰余計算を通す意味がないから使わない。
    let mic = shogun_core::audio::capture::mic::Mic::open().map_err(|e| {
        eprintln!("[ptt] microphone unavailable ({e})");
        Fail::MicUnavailable
    })?;

    let sink = Arc::new(Mutex::new(BufferSink::new()));
    let sink_for_thread = sink.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = stop.clone();

    let mut worker = Worker::new(mic, asr);
    let join = std::thread::spawn(move || {
        while !stop_flag.load(Ordering::Relaxed) {
            let consumed = match sink_for_thread.lock() {
                Ok(g) => worker.poll(now_ms(), &mut Locked(g)),
                // ロックが毒された = 別スレッドがpanicした。マイクを開いたまま回り続けない。
                Err(_) => break,
            };
            if consumed == 0 {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        // 最後の発話を吐き出してデバイスを解放する。
        if let Ok(g) = sink_for_thread.lock() {
            worker.stop(now_ms(), &mut Locked(g));
        }
    });

    eprintln!("[ptt] audio lane started");
    Ok(Handle { stop, join: Some(join), sink })
}

/// マイクを閉じ、溜まった文字起こしを取り出す。
pub fn stop(handle: Handle) -> String {
    let mut handle = handle;
    join_lane(&mut handle);
    handle.sink.lock().map(|mut s| s.take()).unwrap_or_default()
}

/// マイクを閉じ、溜まったものを捨てる。誤爆とキャンセルの道。
pub fn discard(handle: Handle) {
    let mut handle = handle;
    join_lane(&mut handle);
    if let Ok(mut s) = handle.sink.lock() {
        s.discard();
    }
    eprintln!("[ptt] audio lane discarded");
}

/// 停止フラグを立ててポーリングスレッドの終了を待つ。`join` を `lock` より先に済ませるのが
/// 肝: スレッドは終了間際に `worker.stop` の中で同じ `Mutex` を取って最後の発話を flush する。
/// もし停止側が先に lock を握ると、スレッドの flush がその lock 待ちで止まり、`join` は
/// 永遠に返らずデッドロックする。join を待ってから lock を取れば、flush は既に終わっていて
/// ロックは空いており、`take`/`discard` は最終状態のバッファを確実に見る。
fn join_lane(handle: &mut Handle) {
    handle.stop.store(true, Ordering::Relaxed);
    if let Some(join) = handle.join.take() {
        // panicしたキャプチャスレッドは無視する。どのみち畳んでいる最中で、ここでpanicを
        // 伝播させるとセッション機械ごと落ちる。
        let _ = join.join();
    }
}
