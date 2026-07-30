//! push-to-talk の実行層（Issue #44）。
//!
//! [`shogun_core::ptt::statemachine`] が何をすべきかを決め、ここがそれを実際に行う。
//! マイク・パネル・音・エージェント呼び出しの全てがこのファイルを通るので、
//! 「マイクを開くコードはどこか」の答えが1箇所に収まる。

// state_tag は今のところログ用で未参照。private モジュールなので `pub` でも dead-code 判定を
// 素通りしない。`ptt_lane` / `hold_monitor` / `notch_actions` / `approvals` / `connectors` と
// 同じ idiom。
#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use shogun_core::ptt::statemachine::{
    Effect, Fail, Input, Machine, Panel, Params, Sound, State, Timer,
};
use tauri::{Emitter, Manager};

/// PTTパネルのウィンドウラベル。notch / meeting とは別の窓。
const WINDOW_LABEL: &str = "ptt";

/// パネルのサイズ。録音中は小さく、応答が出たら縦に伸びる。
const LISTENING_SIZE: (f64, f64) = (320.0, 96.0);
const RESPONDING_SIZE: (f64, f64) = (420.0, 260.0);

/// フロントエンドに送る、パネルが見せるべき中身。状態機械の [`Panel`] をそのまま
/// シリアライズせず、UIが読める形に写して渡す。
#[derive(Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PanelView {
    Listening,
    Transcribing,
    Responding,
    Error { code: &'static str },
}

impl From<Panel> for PanelView {
    fn from(p: Panel) -> Self {
        match p {
            Panel::Listening => PanelView::Listening,
            Panel::Transcribing => PanelView::Transcribing,
            Panel::Responding => PanelView::Responding,
            Panel::Error(why) => PanelView::Error { code: why.code() },
        }
    }
}

/// パネルウィンドウを（無ければ）作る。起動時に一度呼び、以降は使い回す。
///
/// meeting overlay と同じ理由で `WebviewUrl::default()` を使う: `App("index.html")` は
/// devサーバーが配らないURLに解決されて、JavaScriptが一度も走らない空の窓になる。
///
/// `float_on_all_spaces` は最後に `orderFrontRegardless` を呼ぶので、作った直後の窓は
/// たとえ `visible(false)` でも一瞬前面に出てしまう。押されるまでは見せたくないので、
/// ここで明示的に `hide()` して伏せる。以降の表示は [`show_panel`] が受け持つ。
pub fn build_panel(app: &tauri::AppHandle) -> Option<tauri::WebviewWindow> {
    if let Some(win) = app.get_webview_window(WINDOW_LABEL) {
        return Some(win);
    }
    let win = tauri::WebviewWindowBuilder::new(app, WINDOW_LABEL, tauri::WebviewUrl::default())
        .title("SHOGUN — voice")
        .transparent(true)
        .decorations(false)
        .resizable(false)
        .always_on_top(true)
        .shadow(false)
        .skip_taskbar(true)
        .inner_size(LISTENING_SIZE.0, LISTENING_SIZE.1)
        .visible(false)
        .focused(false)
        .build()
        .map_err(|e| eprintln!("[ptt] panel window build failed: {e}"))
        .ok()?;
    crate::float_on_all_spaces(&win);
    // orderFrontRegardless が見せた分を伏せ直す。押されるまでは何も出ていない。
    let _ = win.hide();
    eprintln!("[ptt] panel url = {:?}", win.url().map(|u| u.to_string()));
    Some(win)
}

/// パネルに中身を出す。位置は castle 設定に合わせ、録音中も応答も同じ場所に出す
/// （視線を動かさせない）。
pub fn show_panel(app: &tauri::AppHandle, view: PanelView) {
    let Some(win) = app.get_webview_window(WINDOW_LABEL) else { return };
    let size = match view {
        PanelView::Responding => RESPONDING_SIZE,
        _ => LISTENING_SIZE,
    };
    let _ = win.set_size(tauri::LogicalSize::new(size.0, size.1));
    // 状態が変わるたびに送る。webview側はこれだけを見て描き分ける。
    let _ = win.emit("ptt:panel", view);
    let _ = win.show();
    let _ = win.set_always_on_top(true);
    redock_ptt(&win);
}

pub fn hide_panel(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window(WINDOW_LABEL) {
        let _ = win.hide();
    }
}

/// PTTパネルを castle 設定の位置に置き直す。
///
/// `crate::redock_to_castle` は使えない: あれは `overlay_ptr`（＝notchパネル）を動かすので、
/// ここで呼ぶと notch の方が飛ぶ。PTTパネルは別の窓なので、その窓自身の NSWindow を
/// 動かす。座標の意味は redock_to_castle と同じ（可視領域と castle_origin）。
fn redock_ptt(win: &tauri::WebviewWindow) {
    let ptr = match win.ns_window() {
        Ok(p) if !p.is_null() => p as *mut objc2::runtime::AnyObject,
        _ => return,
    };
    // SAFETY: `ptr` は tauri が所有する生きた NSWindow。show() は main スレッドから呼ばれる
    // 前提（Task 12 の実行層が main で回す）で、ここも同じ経路で走る。setter は void を返す
    // 純粋な AppKit 呼び出し。
    unsafe {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        use objc2_foundation::{NSPoint, NSRect};
        use shogun_core::notch::geometry::{castle_origin, Rect as GRect};
        let screen: *mut AnyObject = msg_send![ptr, screen];
        if screen.is_null() {
            return;
        }
        let vf: NSRect = msg_send![screen, visibleFrame];
        let w: NSRect = msg_send![ptr, frame];
        let vis = GRect::new(vf.origin.x, vf.origin.y, vf.size.width, vf.size.height);
        let o = castle_origin(vis, w.size.width, w.size.height, crate::current_castle());
        let origin = NSPoint { x: o.x, y: o.y };
        let _: () = msg_send![ptr, setFrameOrigin: origin];
    }
}

/// 開始・終了の合図。macOS標準のシステムサウンドを使うので、ユーザーのシステム音量と
/// 「サウンドエフェクトを再生」設定にそのまま従う。独自の音源ファイルは持たない。
pub fn play_sound(sound: Sound) {
    let name = match sound {
        Sound::Start => "Tink",
        Sound::End => "Pop",
    };
    // SAFETY: `NSSound` の再生は独自のオーディオスレッドに投げられるので、この
    // `soundNamed:` + `play` はどのスレッドから呼んでも安全（新しい NSWindow を作るような
    // main-thread-only の API ではない）。失敗しても無視する — 音が出ないことでセッションを
    // 止める理由がない。
    unsafe {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};
        use objc2_foundation::NSString;
        let ns_name = NSString::from_str(name);
        let sound: *mut AnyObject = msg_send![class!(NSSound), soundNamed: &*ns_name];
        if !sound.is_null() {
            let _: bool = msg_send![sound, play];
        }
    }
}

/// 現在の状態タグ。ログとデバッグ用。
pub fn state_tag(state: State) -> &'static str {
    state.tag()
}

/// 失敗理由からユーザーに見せる一文を作る。**英語**（v1規約）。i18n-readyに保つため、
/// 文言はこの関数だけに集める。
pub fn fail_message(why: Fail) -> &'static str {
    match why {
        Fail::MicUnavailable => {
            "SHOGUN cannot reach the microphone. Open Privacy & Security settings to allow it."
        }
        Fail::NoAsrModel => "The speech model is not available yet.",
        Fail::NothingHeard => "Nothing was heard. Hold the key and speak, then let go.",
        Fail::AsrFailed => "That could not be transcribed. Try once more.",
        Fail::Network => "SHOGUN could not reach the network.",
        Fail::KeyRejected => "The API key was rejected. Check it in Settings.",
    }
}

/// プロセス起動を原点とする単調時計。hold の長さ計算に使う。
///
/// 壁時計だと、録音中に NTP 補正や手動の時刻変更が入ると hold が負になり得て、機械の
/// `min_hold_ms` 誤爆判定が壊れる。`Instant` は単調増加が保証されるのでそれが起きない。
pub fn mono_ms() -> i64 {
    static ORIGIN: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    ORIGIN.get_or_init(Instant::now).elapsed().as_millis() as i64
}

/// hold の押し下げ/離しを、単調時計の刻印付き `Input` にして返す。`lib.rs` の hold monitor
/// コールバックから使う小さな入口で、statemachine の型を lib.rs に晒さずに済ませる。
pub fn mono_input_hold_start() -> Input {
    Input::HoldStart { at_ms: mono_ms() }
}
pub fn mono_input_hold_end() -> Input {
    Input::HoldEnd { at_ms: mono_ms() }
}

/// 実行層の全状態。Tauri state として1つだけ持つ。
pub struct Session {
    machine: Mutex<Machine>,
    /// 動作中のASRレーン。`Recording` のときだけ `Some`。
    lane: Mutex<Option<crate::ptt_lane::Handle>>,
    /// 上限タイマーの世代。キャンセルは「世代を進める」ことで行う — 起動済みのスリープを
    /// 止める術がないので、目覚めたスレッドに自分が古いことを気づかせる。
    max_hold_epoch: Arc<AtomicU64>,
    /// このセッションでマイクが開いた時刻。計測用。
    started_at: Mutex<Option<Instant>>,
}

impl Session {
    pub fn new() -> Self {
        Session {
            machine: Mutex::new(Machine::new(Params::default())),
            lane: Mutex::new(None),
            max_hold_epoch: Arc::new(AtomicU64::new(0)),
            started_at: Mutex::new(None),
        }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// 使うASRモデルと言語。PTTは会議とは独立なので、既定（Small / English）を使う。Task 13 で
/// 設定可能にするまでの暫定値。
fn asr_choice() -> (shogun_core::meeting::settings::AsrModel, shogun_core::meeting::settings::MeetingLanguage) {
    (
        shogun_core::meeting::settings::AsrModel::default(),
        shogun_core::meeting::settings::MeetingLanguage::default(),
    )
}

/// 機械に入力を1つ与え、返った副作用を順番に実行する。**全ての入力はここを通る** ので、
/// 機械のテストが実挙動をそのまま説明する。
///
/// ロックは step の間だけ握って、副作用を回す前に必ず落とす。`StartCapture` の失敗経路は
/// `feed` を再入するので、機械ロックを握ったまま副作用を回すと自分自身とデッドロックする。
pub fn feed(app: &tauri::AppHandle, input: Input) {
    let session = app.state::<Session>();
    let effects = {
        let Ok(mut m) = session.machine.lock() else {
            eprintln!("[ptt] machine lock poisoned — dropping input");
            return;
        };
        m.step(input)
        // ここでロックが落ちる。以降の run_effect / feed 再入は機械ロックを持たない。
    };
    for effect in effects {
        run_effect(app, effect);
    }
}

fn run_effect(app: &tauri::AppHandle, effect: Effect) {
    let session = app.state::<Session>();
    match effect {
        Effect::Transition(s) => eprintln!("[ptt] → {}", s.tag()),

        Effect::StartCapture => {
            let (model, language) = asr_choice();
            match crate::ptt_lane::start(app, model, language) {
                Ok(handle) => {
                    if let Ok(mut g) = session.lane.lock() {
                        *g = Some(handle);
                    }
                    if let Ok(mut g) = session.started_at.lock() {
                        *g = Some(Instant::now());
                    }
                    crate::analytics::capture_ptt_started(app);
                }
                // 機械に遷移を決めさせる。ここで state を触らない。
                Err(why) => feed(app, Input::Failed(why)),
            }
        }

        // whisperは数百ms掛かる。イベントハンドラのスレッドで待つとhold monitorが凍るので、
        // 停止と文字起こしの取り出しは新しいスレッドで。
        Effect::StopCapture => {
            let handle = session.lane.lock().ok().and_then(|mut g| g.take());
            if let Ok(mut g) = session.started_at.lock() {
                *g = None;
            }
            if let Some(handle) = handle {
                let app = app.clone();
                std::thread::spawn(move || {
                    let text = crate::ptt_lane::stop(handle);
                    feed(&app, Input::Transcribed(text));
                });
            }
        }

        Effect::DiscardCapture => {
            let handle = session.lane.lock().ok().and_then(|mut g| g.take());
            if let Ok(mut g) = session.started_at.lock() {
                *g = None;
            }
            if let Some(handle) = handle {
                // 捨てるだけでも whisper のバッファ解放でブロックし得るのでスレッドに逃がす。
                std::thread::spawn(move || crate::ptt_lane::discard(handle));
            }
        }

        Effect::PlaySound(s) => play_sound(s),

        Effect::ShowPanel(p) => {
            if let Panel::Error(why) = p {
                if let Some(win) = app.get_webview_window(WINDOW_LABEL) {
                    let _ = win.emit(
                        "ptt:error",
                        serde_json::json!({ "code": why.code(), "message": fail_message(why) }),
                    );
                }
                crate::analytics::capture_ptt_failed(app, why.code());
            }
            show_panel(app, p.into());
        }

        Effect::HidePanel => hide_panel(app),

        Effect::SubmitToAgent(text) => submit(app, text),

        Effect::StartTimer { timer: Timer::MaxHold, ms } => {
            let epoch = session.max_hold_epoch.fetch_add(1, Ordering::SeqCst) + 1;
            let epoch_ref = session.max_hold_epoch.clone();
            let app = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(ms));
                // 起きたときにまだ自分の世代なら発火。キャンセルされていれば世代がずれていて、
                // 古いタイマーは黙って消える。
                if epoch_ref.load(Ordering::SeqCst) == epoch {
                    feed(&app, Input::MaxHoldExpired { at_ms: mono_ms() });
                }
            });
        }

        Effect::CancelTimer(Timer::MaxHold) => {
            // 世代を進める。走っているスリープは目覚めたときに自分が古いと気づく。
            session.max_hold_epoch.fetch_add(1, Ordering::SeqCst);
        }
    }
}

/// 文字起こしテキストをコンテキストと合わせてエージェントへ投げ、応答をストリームで
/// パネルに流す。**新しいスレッドで走る** — ネットワーク往復でイベントスレッドを塞がない。
fn submit(app: &tauri::AppHandle, spoken: String) {
    use shogun_core::daemon::{Db, ReplyContextCache};
    use shogun_core::ptt::prompt::{build_prompt, Spoken};

    let app = app.clone();
    std::thread::spawn(move || {
        // DBはエージェント構築（Keychainキーの解決）に要る。無ければ投げられない。
        let Some(db) = app.try_state::<Db>().map(|s| s.inner().clone()) else {
            feed(&app, Input::Failed(Fail::Network));
            return;
        };

        // 温まったコンテキストは**読むだけ**。押した瞬間に組み立てるとSLOを割るので、冷えて
        // いれば facts は空スライスのまま先へ進む（待たない）。
        let facts: Vec<String> = app
            .try_state::<ReplyContextCache>()
            .and_then(|c| c.current())
            .map(|ctx| ctx.facts)
            .unwrap_or_default();

        let (front_app, window_title) = foreground_app_and_title();
        let prompt = build_prompt(
            &spoken,
            &Spoken {
                app: front_app.as_deref(),
                window_title: window_title.as_deref(),
                facts: &facts,
            },
        );

        let Some(agent) = crate::inline_source::mac::build_agent(&db) else {
            feed(&app, Input::Failed(Fail::KeyRejected));
            return;
        };

        // 送信は別スレッド、受信はこのスレッド。デルタが届くたびに `ptt` ウィンドウへ流す。
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        let prompt_for_send = prompt.clone();
        let sender = std::thread::spawn(move || agent.complete_streaming_blocking(&prompt_for_send, tx));

        let started = Instant::now();
        let mut first_token_ms: Option<u64> = None;
        let win = app.get_webview_window(WINDOW_LABEL);
        for delta in rx {
            if first_token_ms.is_none() {
                let ms = started.elapsed().as_millis() as u64;
                first_token_ms = Some(ms);
                // 初トークンだけ SloRegister に記録（SLO-03: 初トークン1s）。setup が早期returnで
                // manage前に抜けている可能性があるので try_state。
                if let Some(reg) = app.try_state::<crate::metrics::SloRegister>() {
                    reg.record_first_token_ms(ms as f64);
                }
            }
            if let Some(w) = win.as_ref() {
                let _ = w.emit("ptt:delta", delta);
            }
        }

        // 送信スレッドの結果を回収。panic は Network 失敗として扱う（詳細は言わない）。
        match sender.join() {
            Ok(Ok(())) => {
                let total_ms = started.elapsed().as_millis() as u64;
                crate::analytics::capture_ptt_completed(&app, first_token_ms.unwrap_or(0), total_ms);
                feed(&app, Input::ResponseDone);
            }
            Ok(Err(why)) => feed(&app, Input::Failed(why)),
            Err(_) => feed(&app, Input::Failed(Fail::Network)),
        }
    });
}

/// いま前面にあるアプリの表示名とウィンドウタイトル。
///
/// どちらも取れなくてよい。プロンプトに**添える**情報であって、無いことが発話を投げない
/// 理由にはならない。AXの読み取りをここで新しく書かない — `capture_source.rs` の
/// フォーカス取得と同じ部品を使う。
fn foreground_app_and_title() -> (Option<String>, Option<String>) {
    let Some(front) = crate::display::frontmost_app() else {
        return (None, None);
    };
    let name = (!front.name.is_empty()).then(|| front.name.clone());
    let title = crate::axcache::focused_window(front.pid).and_then(|w| w.title());
    (name, title)
}

/// Esc / パネルのキャンセル操作。録音中なら捨てて閉じる。
#[tauri::command]
pub fn ptt_cancel(app: tauri::AppHandle) {
    feed(&app, Input::Cancel);
}

/// パネルを閉じる（失敗表示や読み終わった応答を片付ける）。
#[tauri::command]
pub fn ptt_dismiss(app: tauri::AppHandle) {
    feed(&app, Input::Dismiss);
}

/// パネルから Full UI を開く。既存の窓生成経路をそのまま使う。
#[tauri::command]
pub fn ptt_open_full_ui(app: tauri::AppHandle) {
    crate::build_full_ui_window(&app);
}

/// マイク権限の設定ペインを開く。`MicUnavailable` のエラー文からの導線。
#[tauri::command]
pub fn ptt_open_privacy_settings() {
    let url = "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone";
    if let Err(e) = std::process::Command::new("open").arg(url).spawn() {
        eprintln!("[ptt] failed to open privacy settings: {e}");
    }
}
