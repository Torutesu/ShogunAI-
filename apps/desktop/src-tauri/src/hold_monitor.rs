//! 素の修飾キーの長押し検知（Issue #44）。
//!
//! `watch_option_tap` と同じ NSEvent グローバルモニタの上に立つが、見ているものが逆:
//! あちらは「短く単独で叩いた」を、こちらは「押している間ずっと」を取る。
//!
//! 素の修飾キーである理由は、tauriのグローバルショートカットが素の修飾キーを登録できない
//! から（`watch_option_tap` の冒頭コメントと同じ制約）。そして素の修飾キーを選ぶ理由は、
//! 長押しに文字キーを混ぜると押している間ずっとキーリピートが走り、前面アプリに文字が
//! 流れ込むから。
//!
//! 既定は右⌘。macOSが右⌘単独に何も割り当てておらず、⌘Space（Spotlight）とも衝突しない。
//! ⌥単独は既存のdraftトリガ（`watch_option_tap`）が使っているので選べない。
//!
//! `watch` はまだどこからも呼ばれていない（配線はTask 12）。それまでのdead-code警告は、
//! 配線前のモジュールと同じ扱い（`notch_actions` 等）でモジュール単位に抑止する。
#![allow(dead_code)]

/// 長押しに使える素の修飾キー。左側のキーは意図的に含めない — 通常のショートカットの
/// 起点として最も使われるので、長押し判定と取り合いになる。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HoldKey {
    #[default]
    RightCommand,
    RightOption,
    Fn,
}

impl HoldKey {
    /// 設定ファイルに書く安定した文字列。
    pub fn key(self) -> &'static str {
        match self {
            HoldKey::RightCommand => "right_command",
            HoldKey::RightOption => "right_option",
            HoldKey::Fn => "fn",
        }
    }

    pub fn from_key(s: &str) -> Option<Self> {
        match s {
            "right_command" => Some(HoldKey::RightCommand),
            "right_option" => Some(HoldKey::RightOption),
            "fn" => Some(HoldKey::Fn),
            _ => None,
        }
    }

    /// このキーの `NSEvent.keyCode`。左右の判別はこれでしかできない — `modifierFlags`
    /// は左右を区別しない。
    fn key_code(self) -> u16 {
        match self {
            HoldKey::RightCommand => 54,
            HoldKey::RightOption => 61,
            HoldKey::Fn => 63,
        }
    }

    /// このキーが押されているときに立つ `NSEventModifierFlags` のビット。
    fn flag(self) -> usize {
        match self {
            HoldKey::RightCommand => 1 << 20, // NSEventModifierFlagCommand
            HoldKey::RightOption => 1 << 19,  // NSEventModifierFlagOption
            HoldKey::Fn => 1 << 23,           // NSEventModifierFlagFunction
        }
    }
}

/// 長押しの監視を開始する。アプリのライフタイム中ずっと動き続ける（モニタは意図的にleakする、
/// `watch_option_tap` と同じ）。
///
/// 押し下がったら `on_start`、離れたら `on_end` を呼ぶ。**`on_end` は `on_start` を呼んだ
/// 場合にのみ呼ばれる** — 押していないキーが離れたことにして、開いていないマイクを閉じに
/// 行かせない。
///
/// 他のキーやマウスが割り込んだholdは無効化する（poison）。⌘クリックや⌘Tabを
/// 「長押し」と読み違えると、ユーザーが普通の操作をしただけで録音が始まる。
#[cfg(target_os = "macos")]
pub fn watch<S, E>(key: HoldKey, on_start: S, on_end: E)
where
    S: Fn() + Send + Sync + 'static,
    E: Fn() + Send + Sync + 'static,
{
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    /// このholdでマイクを開いたか。`on_end` を呼んで良いかの唯一の判断材料。
    static HOLDING: AtomicBool = AtomicBool::new(false);
    /// 他の入力が割り込んだ。キーが完全に離れるまで再武装しない — poison中は押し下げエッジを
    /// 見ても `on_start` を呼ばない。
    static POISONED: AtomicBool = AtomicBool::new(false);
    /// 前回の flagsChanged 時点でこのキーが押されていたか。真の押し下げエッジだけを取るため。
    static WAS_DOWN: AtomicBool = AtomicBool::new(false);

    const MASK_KEY_DOWN: usize = 1 << 10; // NSEventMaskKeyDown
    const MASK_FLAGS_CHANGED: usize = 1 << 12; // NSEventMaskFlagsChanged
    // ⌘クリック・⌘ドラッグ・⌘スクロールを長押しと読まないための、マウス系の全マスク。
    // `watch_option_tap` の MASK_MOUSE と同じ集合。
    const MASK_MOUSE: usize = (1 << 1)
        | (1 << 2)
        | (1 << 3)
        | (1 << 4)
        | (1 << 5)
        | (1 << 6)
        | (1 << 22)
        | (1 << 25)
        | (1 << 26)
        | (1 << 27)
        | (1 << 29)
        | (1 << 30)
        | (1 << 31);

    let target_code = key.key_code();
    let target_flag = key.flag();
    // 対象キー以外の修飾キー。長押し中にこれらが加わったら和音であって長押しではない。
    const ALL_MODIFIERS: usize = (1 << 17) | (1 << 18) | (1 << 19) | (1 << 20) | (1 << 23);
    let other_modifiers = ALL_MODIFIERS & !target_flag;

    let on_start = Arc::new(on_start);
    let on_end = Arc::new(on_end);

    // 割り込みでholdを無効化する。すでに録音が始まっていたなら、開いたマイクは閉じる。
    let poison = {
        let on_end = on_end.clone();
        Arc::new(move || {
            POISONED.store(true, Ordering::Relaxed);
            if HOLDING.swap(false, Ordering::Relaxed) {
                on_end();
            }
        })
    };

    // SAFETY: setup（メインスレッド）から呼ぶ。モニタとブロックはアプリのライフタイム分
    // 意図的にleakする（`watch_option_tap` と同じ扱い）。
    unsafe {
        let poison_for_block = poison.clone();
        let disarm_block = block2::RcBlock::new(move |_ev: *mut AnyObject| poison_for_block());
        let key_mon: *mut AnyObject = msg_send![
            class!(NSEvent),
            addGlobalMonitorForEventsMatchingMask: MASK_KEY_DOWN,
            handler: &*disarm_block
        ];
        let mouse_mon: *mut AnyObject = msg_send![
            class!(NSEvent),
            addGlobalMonitorForEventsMatchingMask: MASK_MOUSE,
            handler: &*disarm_block
        ];
        std::mem::forget(disarm_block);
        let _ = (key_mon, mouse_mon);

        let flags_block = block2::RcBlock::new(move |ev: *mut AnyObject| {
            if ev.is_null() {
                return;
            }
            let code: u16 = msg_send![ev, keyCode];
            let flags: usize = msg_send![ev, modifierFlags];

            // 対象キー以外の修飾キーが動いた場合: それが押し下げなら和音なので無効化する。
            if code != target_code {
                if flags & other_modifiers != 0 {
                    poison();
                }
                return;
            }

            let down = flags & target_flag != 0;
            let was_down = WAS_DOWN.swap(down, Ordering::Relaxed);

            if down && !was_down {
                // 真の押し下げエッジ。他の修飾キーが既に押されているなら和音なので始めない。
                if flags & other_modifiers != 0 {
                    POISONED.store(true, Ordering::Relaxed);
                    return;
                }
                // poison中（割り込み後、まだ完全に離れていない）は再武装しない。素の押し下げ
                // エッジは真の up→down でしか来ないので通常ここは false だが、念のため守る。
                if POISONED.load(Ordering::Relaxed) {
                    return;
                }
                if !HOLDING.swap(true, Ordering::Relaxed) {
                    on_start();
                }
            } else if !down && was_down {
                // 完全に離れた。ここが唯一の再武装ポイント。
                POISONED.store(false, Ordering::Relaxed);
                if HOLDING.swap(false, Ordering::Relaxed) {
                    on_end();
                }
            }
        });
        let flags_mon: *mut AnyObject = msg_send![
            class!(NSEvent),
            addGlobalMonitorForEventsMatchingMask: MASK_FLAGS_CHANGED,
            handler: &*flags_block
        ];
        std::mem::forget(flags_block);
        let _ = flags_mon;
    }

    eprintln!("[ptt] hold monitor watching {}", key.key());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_hold_key_is_right_command() {
        assert_eq!(HoldKey::default(), HoldKey::RightCommand);
    }

    /// 設定ファイルとの往復で値が変わらない。
    #[test]
    fn hold_keys_round_trip_through_their_wire_key() {
        for k in [HoldKey::RightCommand, HoldKey::RightOption, HoldKey::Fn] {
            assert_eq!(HoldKey::from_key(k.key()), Some(k));
        }
    }

    #[test]
    fn an_unknown_wire_key_is_rejected_rather_than_guessed() {
        assert_eq!(HoldKey::from_key("left_command"), None);
        assert_eq!(HoldKey::from_key(""), None);
    }

    /// 左⌘(55)を拾わない。通常のショートカットの起点と取り合いになる。
    #[test]
    fn the_right_command_key_code_is_not_the_left_one() {
        assert_eq!(HoldKey::RightCommand.key_code(), 54);
        assert_ne!(HoldKey::RightCommand.key_code(), 55);
    }
}
