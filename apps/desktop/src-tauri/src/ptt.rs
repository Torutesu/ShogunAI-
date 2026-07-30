//! push-to-talk の実行層（Issue #44）。
//!
//! [`shogun_core::ptt::statemachine`] が何をすべきかを決め、ここがそれを実際に行う。
//! マイク・パネル・音・エージェント呼び出しの全てがこのファイルを通るので、
//! 「マイクを開くコードはどこか」の答えが1箇所に収まる。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::hold_monitor::HoldKey;

use shogun_core::ptt::statemachine::{
    Effect, Fail, Input, Machine, Panel, Params, Sound, Timer,
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

/// PTTの設定。秘密は含まないので Keychain は不要。`app_data/ptt.json` に平文で置く。
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct PttSettings {
    /// β機能なので既定はオフ。設定から明示的に有効化する。
    #[serde(default)]
    pub enabled: bool,
    /// 長押しに使うキーの安定文字列（`HoldKey::key()`）。
    #[serde(default = "default_hold_key")]
    pub hold_key: String,
}

fn default_hold_key() -> String {
    HoldKey::default().key().to_string()
}

impl Default for PttSettings {
    fn default() -> Self {
        PttSettings { enabled: false, hold_key: default_hold_key() }
    }
}

/// βフラグ。長押し監視は常に張るが、無効なら押し下げを捨てる — キー変更の反映には
/// 再起動が要る代わりに、有効/無効の切り替えは即座に効く。
pub static ENABLED: AtomicBool = AtomicBool::new(false);

fn settings_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("ptt.json"))
}

/// 設定を読む。読めない・壊れているときは既定に落ちる — **壊れた設定ファイルが
/// マイクを開いたままにしてはならない**ので、既定は必ず enabled=false 側。
pub fn load_settings(app: &tauri::AppHandle) -> PttSettings {
    let Some(path) = settings_path(app) else {
        return PttSettings::default();
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return PttSettings::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// 設定を書く。整形済みJSONで、人が開いて読める形に。
pub fn save_settings(app: &tauri::AppHandle, settings: &PttSettings) -> Result<(), String> {
    let path = settings_path(app).ok_or("app_data_dir unavailable")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// 現在の設定を返す。
#[tauri::command]
pub fn get_ptt_settings(app: tauri::AppHandle) -> PttSettings {
    load_settings(&app)
}

/// 設定を保存する。`hold_key` が未知なら拒否する — 保存を許すと再起動後に
/// `from_key` が既定へ黙って落ち、ユーザーが選んだつもりのキーと食い違う。
/// `enabled` は即座に `ENABLED` へ反映するが、`hold_key` は監視の張り直しに再起動が要る。
#[tauri::command]
pub fn set_ptt_settings(app: tauri::AppHandle, settings: PttSettings) -> Result<(), String> {
    if HoldKey::from_key(&settings.hold_key).is_none() {
        return Err(format!("unknown hold_key: {}", settings.hold_key));
    }
    save_settings(&app, &settings)?;
    ENABLED.store(settings.enabled, Ordering::Relaxed);
    // 結果を1行残す（`castle::set_castle_position` と同じ idiom）。`enabled` はこの瞬間から
    // 効くが `hold_key` は次回起動から、という非対称を明示しておくと、実機で「キーを変えたのに
    // 効かない」を見たとき原因が設定の未保存でなく再起動待ちだと切り分けられる。
    eprintln!(
        "[ptt] settings saved: enabled={} (live), hold_key={} (after restart)",
        settings.enabled, settings.hold_key
    );
    Ok(())
}

/// 機械に入力を1つ与え、返った副作用を順番に実行する。**全ての入力はここを通る** ので、
/// 機械のテストが実挙動をそのまま説明する。
///
/// ロックは step の間だけ握って、副作用を回す前に必ず落とす。`StartCapture` の失敗経路は
/// `feed` を再入するので、機械ロックを握ったまま副作用を回すと自分自身とデッドロックする。
pub fn feed(app: &tauri::AppHandle, input: Input) {
    // βで無効なときは `HoldStart` だけを捨てる。**それ以外は必ず通す** — 録音中に設定を
    // オフにしても、`Cancel` / `HoldEnd` / `MaxHoldExpired` は届いてマイクを閉じねばならない。
    // この非対称が肝で、全部捨てると押しっぱなしで無効化したときマイクが開きっぱなしになる。
    if matches!(input, Input::HoldStart { .. }) && !ENABLED.load(Ordering::Relaxed) {
        return;
    }

    let session = app.state::<Session>();

    // hold→パネルの計測起点。`HoldStart` のときだけ時刻を控え、`ShowPanel(Listening)` が
    // 出るまでの実測を SLO（パネル展開≤100ms）に記録する。ノッチ展開と同じ種類の窓・同じ問い
    // なので、専用メトリクスを足さず既存の expand を再利用する。
    let hold_start = matches!(input, Input::HoldStart { .. }).then(Instant::now);

    let effects = {
        let Ok(mut m) = session.machine.lock() else {
            eprintln!("[ptt] machine lock poisoned — dropping input");
            return;
        };
        m.step(input)
        // ここでロックが落ちる。以降の run_effect / feed 再入は機械ロックを持たない。
    };

    let shows_listening = effects
        .iter()
        .any(|e| matches!(e, Effect::ShowPanel(Panel::Listening)));

    for effect in effects {
        run_effect(app, effect);
    }

    // `show_panel` は同期なので、効果ループを抜けた時点でパネルは出ている。押下から
    // ここまでの経過が hold→パネルの実測。setup が manage 前に早期returnし得るので try_state。
    if let (Some(started), true) = (hold_start, shows_listening) {
        if let Some(reg) = app.try_state::<crate::metrics::SloRegister>() {
            reg.record_expand_ms(started.elapsed().as_millis() as f64);
        }
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
                crate::analytics::capture_ptt_completed(&app, first_token_ms, total_ms);
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
