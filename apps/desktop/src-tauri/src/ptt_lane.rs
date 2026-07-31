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
//! 存在せず、ここが受け取るのは文字起こし後のテキストだけ。キャッシュするのはモデルの重み
//! （`Whisper`）であって音声ではない。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use shogun_core::audio::asr::whisper::Whisper;
use shogun_core::audio::worker::{SegmentSink, Worker};
use shogun_core::audio::Utterance;
use shogun_core::meeting::settings::{AsrModel, MeetingLanguage};
use shogun_core::ptt::buffer_sink::BufferSink;
use shogun_core::ptt::statemachine::{Fail, Input};

/// 動作中の一発レーン。`stop` か `discard` で必ず回収する。
pub struct Handle {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    /// ポーリングスレッドと停止側の両方から触るので `Mutex`。中身はテキストだけ。
    sink: Arc<Mutex<BufferSink>>,
}

/// キャッシュ1件: どのモデル（パス）・どの言語で読んだ `Whisper` か。キーの2要素で「同じ
/// モデルか」を判定し、一致するときだけ再利用する。
type CachedModel = (PathBuf, Option<String>, Whisper);

/// ロード済みモデルの再利用キャッシュ。PTTは毎セッション同じモデル（既定 Small / English）を
/// 開くので、`WhisperContext`（数百MB）のロードを毎回払わずに済ませる。キーはパスと言語コード:
/// どちらかが変わったら別モデルなので破棄して読み直す。**キャッシュするのは重みだけで、音声は
/// 一切入らない**（不変条件2）。会議レーンはこのキャッシュに触れないので、その挙動は変わらない。
static MODEL_CACHE: OnceLock<Mutex<Option<CachedModel>>> = OnceLock::new();

fn model_cache() -> &'static Mutex<Option<CachedModel>> {
    MODEL_CACHE.get_or_init(|| Mutex::new(None))
}

/// パス・言語が一致するロード済みモデルがあれば取り出す。無ければ（あるいは食い違えば）`None`。
fn take_cached(path: &std::path::Path, lang: Option<&str>) -> Option<Whisper> {
    let mut guard = model_cache().lock().ok()?;
    match guard.as_ref() {
        Some((p, l, _)) if p == path && l.as_deref() == lang => guard.take().map(|(_, _, w)| w),
        // 食い違うキャッシュは今ここで捨てる。次に同じモデルが要求されたときロードし直す。
        _ => {
            *guard = None;
            None
        }
    }
}

/// 使い終わったモデルを次セッションのために戻す。ロックが取れなければ黙って捨てる — 次回は
/// ロードし直すだけで、正しさには影響しない。
fn store_cached(path: PathBuf, lang: Option<String>, asr: Whisper) {
    if let Ok(mut guard) = model_cache().lock() {
        *guard = Some((path, lang, asr));
    }
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
/// **マイクを先に開く。** `Mic::open()` は数十msで返り、開いた瞬間から unbounded channel に
/// フレームが溜まる。一方 `Whisper` のロードは数百MBの重み読み込みで数百ms〜数秒かかるので、
/// これをここ（＝押下から録音開始までの経路）で待つと発話の冒頭が録れない。だからロードは
/// レーンスレッドのポーリング開始前に移し、押下→マイク開までをモデルロードから切り離す。
/// 溜まったフレームはポーリングが追いつくまで channel が保持するので、ロード中の音声は失われ
/// ない（B5）。
///
/// マイク open の失敗（権限拒否 / デバイスなし）は従来どおり `Err(Fail)` で同期返し。モデル
/// ロードの失敗はマイクを開いた**後**にレーンスレッドの中で分かるので、`gen` を控えて
/// [`crate::ptt::feed_if_current`] 経由で `Failed(NoAsrModel)` を返す — 機械は Recording なので、
/// これが届けば DiscardCapture + エラーパネルへ正しく落ちる。
pub fn start(
    app: &tauri::AppHandle,
    gen: u64,
    model: AsrModel,
    language: MeetingLanguage,
) -> Result<Handle, Fail> {
    let Some(model_path) = crate::audio_lane::select_model_path(app, model) else {
        eprintln!("[ptt] no whisper model available");
        return Err(Fail::NoAsrModel);
    };
    let lang: Option<String> = language.whisper_code().map(str::to_string);

    // マイクだけを開く。`Mic` は `AudioSource` を実装しているので、`Worker` にそのまま渡せる。
    // 会議レーンの `MultiSource` はソースが複数あるときのラウンドロビン用で、1本しか無い
    // ここでは剰余計算を通す意味がないから使わない。open は速く、返った瞬間から channel に
    // フレームが溜まり始める。
    let mic = shogun_core::audio::capture::mic::Mic::open().map_err(|e| {
        eprintln!("[ptt] microphone unavailable ({e})");
        Fail::MicUnavailable
    })?;

    let sink = Arc::new(Mutex::new(BufferSink::new()));
    let sink_for_thread = sink.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = stop.clone();
    let app = app.clone();

    let join = std::thread::spawn(move || {
        // 重みのロードはここ（ポーリングの前）で払う。マイクは既に開いていてフレームは
        // channel に溜まっているので、この数百msの間の音声は失われない。まずキャッシュを見て、
        // 同じモデルが残っていれば読み直さない。
        let (lang_code, path_for_load) = (lang.as_deref(), model_path.clone());
        let asr = match take_cached(&path_for_load, lang_code) {
            Some(cached) => cached,
            None => match Whisper::load_with_language(&path_for_load.to_string_lossy(), lang_code) {
                Ok(w) => w,
                Err(e) => {
                    eprintln!("[ptt] whisper load failed ({e})");
                    // マイクは開いたまま。機械は Recording なので、Failed が届けば
                    // DiscardCapture でマイクを閉じてエラーパネルへ落ちる。
                    crate::ptt::feed_if_current(&app, gen, Input::Failed(Fail::NoAsrModel));
                    return;
                }
            },
        };

        let mut worker = Worker::new(mic, asr);
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
        // 重みは次セッションのために返す。音声は `worker.stop` で既に flush・破棄済みで、
        // `into_asr` が返すのは `Whisper`（重み）だけ。
        store_cached(model_path, lang, worker.into_asr());
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
