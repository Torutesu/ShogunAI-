//! push-to-talk の実行層（Issue #44）。
//!
//! [`shogun_core::ptt::statemachine`] が何をすべきかを決め、ここがそれを実際に行う。
//! マイク・パネル・音・エージェント呼び出しの全てがこのファイルを通るので、
//! 「マイクを開くコードはどこか」の答えが1箇所に収まる。

// state_tag / fail_message は Task 12 で状態機械の実行に配線されるまで呼ばれない。private かつ
// 未参照のモジュールなので、`pub` な項目も dead-code 判定を素通りしない。`ptt_lane` /
// `hold_monitor` / `notch_actions` / `approvals` / `connectors` と同じ idiom。配線後は外せる。
#![allow(dead_code)]

use shogun_core::ptt::statemachine::{Fail, Panel, Sound, State};
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
