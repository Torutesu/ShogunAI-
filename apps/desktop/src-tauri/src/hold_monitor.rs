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

    /// このキーが押されているかを判定するビット。左右を区別する device-dependent マスク
    /// （`IOKit/IOLLEvent.h`）を使う — 汎用の `NSEventModifierFlagCommand` は左右で共有
    /// されるので、左⌘を押したまま右⌘を離しても「まだ押されている」と読めてしまい、
    /// 離したことに気づけない（マイクが開いたまま残る）。
    fn down_flag(self) -> usize {
        match self {
            HoldKey::RightCommand => 0x0000_0010, // NX_DEVICERCMDKEYMASK
            HoldKey::RightOption => 0x0000_0040,  // NX_DEVICERALTKEYMASK
            // Fnは1つしか無いので device-dependent ビットが存在せず、汎用ビットで一意。
            HoldKey::Fn => 1 << 23,               // NSEventModifierFlagFunction
        }
    }

    /// 和音判定に使う汎用ビット。こちらは左右をまとめた `NSEventModifierFlags` の側。
    fn generic_flag(self) -> usize {
        match self {
            HoldKey::RightCommand => 1 << 20,
            HoldKey::RightOption => 1 << 19,
            HoldKey::Fn => 1 << 23,
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
    // 押下判定は device-dependent ビットで。左⌘と右⌘は汎用ビットを共有するので、汎用
    // ビットで判定すると左⌘を握ったまま右⌘を離しても「まだ押している」と誤読する。
    let target_down = key.down_flag();
    // 対象キー以外の修飾キー。長押し中にこれらが加わったら和音であって長押しではない。
    // マスクは汎用ビット側で組む（device-dependent ビットは含めない）。
    const ALL_MODIFIERS: usize = (1 << 17) | (1 << 18) | (1 << 19) | (1 << 20) | (1 << 23);
    let other_modifiers = ALL_MODIFIERS & !key.generic_flag();

    // 実機検証用の一時診断（Task 16）。device-dependent ビットが本当に左右を分けるかは
    // ユニットテストでは確かめられない — 実機で `SHOGUN_PTT_DEBUG=1` を立て、この行の
    // hex を見て確認する。既定では読まないので通常コストはゼロ。
    let debug = std::env::var_os("SHOGUN_PTT_DEBUG").is_some();

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

        let flags_block = block2::RcBlock::new(move |ev: *mut AnyObject| {
            if ev.is_null() {
                return;
            }
            let code: u16 = msg_send![ev, keyCode];
            let flags: usize = msg_send![ev, modifierFlags];

            if debug {
                eprintln!("[ptt] flagsChanged code={code:#06x} flags={flags:#010x}");
            }

            if code != target_code {
                // 他の修飾キーが動いた = 和音。フラグの値では判定しない: 左⌘と右⌘は同じ
                // 汎用ビットを共有するので、ビットを見ても「別のキーが動いた」ことは
                // 分からない。keyCode が違う時点で十分な証拠。
                poison();
                return;
            }

            let down = flags & target_down != 0;
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

        // モニタが張れなければ push-to-talk は静かに死ぬ。Accessibility 権限が拒否された
        // ときにログに理由が残るよう、`watch_option_tap` と同じく null を確認する。
        if key_mon.is_null() || mouse_mon.is_null() || flags_mon.is_null() {
            eprintln!("[ptt] hold monitor failed to install (accessibility permission?)");
        } else {
            eprintln!("[ptt] hold monitor watching {}", key.key());
        }
    }
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

    /// 左⌘と右⌘は汎用の Command ビットを共有するので、押下判定には device-dependent
    /// ビットを使う。ここを汎用ビットに戻すと、左⌘を押したまま右⌘を離したときに
    /// 離したことに気づけず、マイクが開いたまま残る。
    #[test]
    fn the_down_flag_distinguishes_left_from_right() {
        assert_eq!(HoldKey::RightCommand.down_flag(), 0x0000_0010);
        assert_eq!(HoldKey::RightOption.down_flag(), 0x0000_0040);
        assert_ne!(
            HoldKey::RightCommand.down_flag(),
            HoldKey::RightCommand.generic_flag(),
            "汎用ビットを押下判定に使っている"
        );
    }

    /// Fn は1つしか無いので device-dependent ビットが要らない。
    #[test]
    fn the_fn_key_uses_its_generic_flag() {
        assert_eq!(HoldKey::Fn.down_flag(), HoldKey::Fn.generic_flag());
    }
}
