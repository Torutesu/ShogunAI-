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
}

/// モニタから見えたイベントに対する判定。押し下げ/離し/割込みという生の事実だけを渡すと、
/// これが「いま何をすべきか」を [`Edge`] で返す。objc ブロックはこの判定を呼んで、返った
/// Edge を on_start/on_end/on_cancel に流すだけ — 状態の全ては、ここに閉じてテスト可能にする。
#[derive(Debug, Default)]
struct HoldState {
    /// このholdでマイクを開いたか。`Edge::End`/`Edge::Cancel` を返して良いかの唯一の判断材料。
    holding: bool,
    /// 他の入力が割り込んだ。対象キーが完全に離れるまで再武装しない — poison中は押し下げ
    /// エッジを見ても `Edge::Start` を返さない。
    poisoned: bool,
    /// 前回の flagsChanged 時点でこのキーが押されていたか。真の押し下げエッジだけを取るため。
    was_down: bool,
}

/// 判定の結果、実行層に伝えるべきこと。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    /// マイクを開く。`on_start`。
    Start,
    /// 手を離した。溜まった録音を文字起こしへ。`on_end`。
    End,
    /// 割込みで潰した。溜まった録音は捨てる。`on_cancel`。
    Cancel,
}

impl HoldState {
    /// keyDown またはマウス系イベント（＝対象キー以外の入力）。**active な hold だけを潰す。**
    ///
    /// hold していないときの割込みでは何もしない。ここで poison を立てると、修飾なしの
    /// タイピング（keyUp を監視していない）のあと毒が残り、次のholdの押し下げエッジが黙って
    /// 無視される — 「1回目は無反応、押し直すと効く」という壊れた挙動になる。poison は
    /// 「active な hold を潰した」ときにだけ立てる（潰したあとは対象キーの完全リリースまで
    /// 再開しないので、そこまで再武装を止める意味がある）。
    fn on_interrupt(&mut self) -> Option<Edge> {
        if self.holding {
            self.holding = false;
            self.poisoned = true;
            Some(Edge::Cancel)
        } else {
            None
        }
    }

    /// flagsChanged イベント。`code`/`flags` はイベントの生の値、`target_*` は監視対象キーの諸元、
    /// `foreign_held` は対象キー以外の修飾キーが押されているかを見るマスク。
    fn on_flags_changed(
        &mut self,
        code: u16,
        flags: usize,
        target_code: u16,
        target_down: usize,
        foreign_held: usize,
    ) -> Option<Edge> {
        if code != target_code {
            if flags & foreign_held != 0 {
                // 他の修飾キーが押されている = 和音。長押しは無効。割込みと同じ扱い
                // （active な hold だけを潰し、hold していなければ何もしない）。
                return self.on_interrupt();
            }
            // 他の修飾キーが「離れた」だけ。押しっぱなしのものが無くなったので、次の
            // 押し下げを受け付けられるよう再武装する。ここで再武装しないと、左⌘を使った
            // あとの最初の長押しが黙って無視される。
            //
            // poison の解除ポイントは非対称に2つある: (1) ここ（foreign が全解放された）と
            // (2) 下の対象キーの完全リリース。片方だけ残すと壊れる — (1) が無いと和音後に
            // 対象キーを離さず foreign だけ離したケースで再武装できず、(2) が無いと単独の
            // 割込み poison が対象キーを離しても解けない。両方を保つこと。
            self.poisoned = false;
            return None;
        }

        let down = flags & target_down != 0;
        let was_down = std::mem::replace(&mut self.was_down, down);

        if down && !was_down {
            // 真の押し下げエッジ。他の修飾キーが既に押されているなら和音なので始めない。
            if flags & foreign_held != 0 {
                self.poisoned = true;
                return None;
            }
            // poison中（割り込み後、まだ完全に離れていない）は再武装しない。素の押し下げ
            // エッジは真の up→down でしか来ないので通常ここは false だが、念のため守る。
            if self.poisoned {
                return None;
            }
            if !self.holding {
                self.holding = true;
                return Some(Edge::Start);
            }
            None
        } else if !down && was_down {
            // 対象キーが完全に離れた。poison 解除の2つ目のポイント（上の foreign 全解放と
            // 対になる非対称）。単独割込みで潰した hold は、対象キーを離すここで初めて解ける。
            self.poisoned = false;
            if self.holding {
                self.holding = false;
                return Some(Edge::End);
            }
            None
        } else {
            None
        }
    }
}

/// 長押しの監視を開始する。アプリのライフタイム中ずっと動き続ける（モニタは意図的にleakする、
/// `watch_option_tap` と同じ）。
///
/// 押し下がったら `on_start`、離れたら `on_end`、割込みで潰されたら `on_cancel` を呼ぶ。
/// **`on_end` または `on_cancel` のどちらか一方が、`on_start` を呼んだ場合にのみ呼ばれる** —
/// 押していないキーが離れたことにして、開いていないマイクを閉じに行かせない。
///
/// 他のキーやマウスが割り込んだholdは無効化する（`on_cancel`）。⌘クリックや⌘Tabを
/// 「長押し」と読み違えると、ユーザーが普通の操作をしただけで録音が始まる。割込みでは
/// `on_end` ではなく `on_cancel` を呼ぶ — `on_end` は文字起こし→送信まで走るので、
/// 「無効化」のつもりの割込みが「送信」に化けてしまう。
#[cfg(target_os = "macos")]
pub fn watch<S, E, C>(key: HoldKey, on_start: S, on_end: E, on_cancel: C)
where
    S: Fn() + Send + Sync + 'static,
    E: Fn() + Send + Sync + 'static,
    C: Fn() + Send + Sync + 'static,
{
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use std::sync::Mutex;

    // 全ての判定はここに閉じる。イベントは全て main スレッドに届くので競合はないが、
    // 生の static ではなく Mutex に包んで unwrap を避ける（毒った lock で panic しない）。
    static STATE: Mutex<HoldState> =
        Mutex::new(HoldState { holding: false, poisoned: false, was_down: false });

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

    /// 修飾キーが「いま押されているか」を左右込みで見るための全ビット
    /// （`IOKit/IOLLEvent.h` の device-dependent マスク + Fn）。
    const ALL_DEVICE_MODIFIERS: usize = 0x0000_0001 // L-Ctrl
        | 0x0000_0002 // L-Shift
        | 0x0000_0004 // R-Shift
        | 0x0000_0008 // L-Cmd
        | 0x0000_0010 // R-Cmd
        | 0x0000_0020 // L-Alt
        | 0x0000_0040 // R-Alt
        | 0x0000_2000 // R-Ctrl
        | (1 << 23); // Fn（device ビットが無い唯一の修飾キー）

    // 対象キー以外の修飾キーが押されているかを見るマスク。device ビットで見るので、
    // 「左⌘を押したまま右⌘」も他が押されていると正しく分かる — 汎用ビットだけでは
    // 左右が同じビットを共有していて区別できなかった。
    let foreign_held = ALL_DEVICE_MODIFIERS & !target_down;

    // 実機検証用の一時診断（Task 16）。device-dependent ビットが本当に左右を分けるかは
    // ユニットテストでは確かめられない — 実機で `SHOGUN_PTT_DEBUG=1` を立て、この行の
    // hex を見て確認する。既定では読まないので通常コストはゼロ。
    let debug = std::env::var_os("SHOGUN_PTT_DEBUG").is_some();

    let on_start = std::sync::Arc::new(on_start);
    let on_end = std::sync::Arc::new(on_end);
    let on_cancel = std::sync::Arc::new(on_cancel);

    // 返った Edge を対応するコールバックに流す。unwrap は使わない — 毒った lock でも
    // デーモンを落とさず、その1イベントを黙って捨てる（CLAUDE.md: デーモンは落とさない）。
    let dispatch_interrupt = {
        let on_cancel = on_cancel.clone();
        move || {
            let Ok(mut s) = STATE.lock() else { return };
            if let Some(Edge::Cancel) = s.on_interrupt() {
                drop(s);
                on_cancel();
            }
        }
    };
    let dispatch_flags = {
        let on_start = on_start.clone();
        let on_end = on_end.clone();
        let on_cancel = on_cancel.clone();
        move |code: u16, flags: usize| {
            let Ok(mut s) = STATE.lock() else { return };
            let edge = s.on_flags_changed(code, flags, target_code, target_down, foreign_held);
            drop(s);
            match edge {
                Some(Edge::Start) => on_start(),
                Some(Edge::End) => on_end(),
                Some(Edge::Cancel) => on_cancel(),
                None => {}
            }
        }
    };

    // SAFETY: setup（メインスレッド）から呼ぶ。モニタとブロックはアプリのライフタイム分
    // 意図的にleakする（`watch_option_tap` と同じ扱い）。
    unsafe {
        // 割込み（keyDown / マウス）用ブロック。グローバル用は戻り値を使わない（フックでは
        // なく傍受なので値を返せない）ので `_ev`、ローカル用はイベントをそのまま返して
        // 通過させる（nil を返すと飲み込んでしまう）。
        let interrupt_global = {
            let f = dispatch_interrupt.clone();
            block2::RcBlock::new(move |_ev: *mut AnyObject| f())
        };
        let interrupt_local = {
            let f = dispatch_interrupt.clone();
            block2::RcBlock::new(move |ev: *mut AnyObject| -> *mut AnyObject {
                f();
                ev
            })
        };
        // flagsChanged 用ブロック。イベントから code/flags を読む。ローカル用は同じ判定を
        // したうえでイベントを返す。
        let flags_global = {
            let f = dispatch_flags.clone();
            block2::RcBlock::new(move |ev: *mut AnyObject| {
                if ev.is_null() {
                    return;
                }
                let code: u16 = msg_send![ev, keyCode];
                let flags: usize = msg_send![ev, modifierFlags];
                if debug {
                    eprintln!("[ptt] flagsChanged code={code:#06x} flags={flags:#010x}");
                }
                f(code, flags);
            })
        };
        let flags_local = {
            let f = dispatch_flags.clone();
            block2::RcBlock::new(move |ev: *mut AnyObject| -> *mut AnyObject {
                if ev.is_null() {
                    return ev;
                }
                let code: u16 = msg_send![ev, keyCode];
                let flags: usize = msg_send![ev, modifierFlags];
                if debug {
                    eprintln!("[ptt] flagsChanged (local) code={code:#06x} flags={flags:#010x}");
                }
                f(code, flags);
                ev
            })
        };

        // グローバルモニタは他アプリ宛のイベントを見る。ローカルモニタは SHOGUN 自身の
        // ウィンドウ（Full UI / 設定）がキーのときのイベントを見る — 設定画面で有効化した
        // 直後や、録音中に自アプリがアクティブ化したときの離しがグローバルには届かないので、
        // 両方を張らないと PTT が無反応・マイクが開きっぱなしになる。
        let key_mon_g: *mut AnyObject = msg_send![
            class!(NSEvent),
            addGlobalMonitorForEventsMatchingMask: MASK_KEY_DOWN,
            handler: &*interrupt_global
        ];
        let key_mon_l: *mut AnyObject = msg_send![
            class!(NSEvent),
            addLocalMonitorForEventsMatchingMask: MASK_KEY_DOWN,
            handler: &*interrupt_local
        ];
        let mouse_mon_g: *mut AnyObject = msg_send![
            class!(NSEvent),
            addGlobalMonitorForEventsMatchingMask: MASK_MOUSE,
            handler: &*interrupt_global
        ];
        let mouse_mon_l: *mut AnyObject = msg_send![
            class!(NSEvent),
            addLocalMonitorForEventsMatchingMask: MASK_MOUSE,
            handler: &*interrupt_local
        ];
        let flags_mon_g: *mut AnyObject = msg_send![
            class!(NSEvent),
            addGlobalMonitorForEventsMatchingMask: MASK_FLAGS_CHANGED,
            handler: &*flags_global
        ];
        let flags_mon_l: *mut AnyObject = msg_send![
            class!(NSEvent),
            addLocalMonitorForEventsMatchingMask: MASK_FLAGS_CHANGED,
            handler: &*flags_local
        ];
        std::mem::forget(interrupt_global);
        std::mem::forget(interrupt_local);
        std::mem::forget(flags_global);
        std::mem::forget(flags_local);

        // モニタが張れなければ push-to-talk は静かに死ぬ。Accessibility 権限が拒否された
        // ときにログに理由が残るよう、`watch_option_tap` と同じく null を確認する。ローカル
        // モニタ側が null でも、自アプリ宛のイベントだけが見えなくなる（他アプリ宛は生きる）
        // ので、6本すべて個別に見る。
        if key_mon_g.is_null()
            || key_mon_l.is_null()
            || mouse_mon_g.is_null()
            || mouse_mon_l.is_null()
            || flags_mon_g.is_null()
            || flags_mon_l.is_null()
        {
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

    /// 左⌘と右⌘は汎用の Command ビット (1<<20) を共有するので、押下判定には
    /// device-dependent ビットを使う。ここを汎用ビットに戻すと、左⌘を押したまま右⌘を
    /// 離したときに離したことに気づけず、マイクが開いたまま残る。
    #[test]
    fn the_down_flag_distinguishes_left_from_right() {
        assert_eq!(HoldKey::RightCommand.down_flag(), 0x0000_0010);
        assert_eq!(HoldKey::RightOption.down_flag(), 0x0000_0040);
        assert_ne!(HoldKey::RightCommand.down_flag(), 1 << 20, "汎用の⌘ビットを押下判定に使っている");
        assert_ne!(HoldKey::RightOption.down_flag(), 1 << 19, "汎用の⌥ビットを押下判定に使っている");
    }

    /// Fn は1つしか無いので device-dependent ビットが存在せず、汎用ビットで一意。
    #[test]
    fn the_fn_key_has_no_device_bit() {
        assert_eq!(HoldKey::Fn.down_flag(), 1 << 23);
    }

    // ── HoldState の判定ロジック ──────────────────────────────────────────────────────
    //
    // 右⌘（既定）の諸元でテストする。実際の値は key_code()/down_flag() から取り、テストが
    // それらの定数を横目に写経しないようにする。

    const TC: u16 = 54; // RightCommand.key_code()
    const TD: usize = 0x0000_0010; // RightCommand.down_flag()
    // 対象キー以外の全修飾ビット（左⌘を含む）。foreign_held と同じ計算。
    const FOREIGN: usize = (0x0000_0001
        | 0x0000_0002
        | 0x0000_0004
        | 0x0000_0008
        | 0x0000_0010
        | 0x0000_0020
        | 0x0000_0040
        | 0x0000_2000
        | (1 << 23))
        & !TD;
    const LEFT_CMD: usize = 0x0000_0008; // NX_DEVICELCMDKEYMASK

    /// 対象キーの真の押し下げエッジ。他修飾は押されていない前提。
    fn press(s: &mut HoldState) -> Option<Edge> {
        s.on_flags_changed(TC, TD, TC, TD, FOREIGN)
    }

    /// 対象キーの真のリリースエッジ。
    fn release(s: &mut HoldState) -> Option<Edge> {
        s.on_flags_changed(TC, 0, TC, TD, FOREIGN)
    }

    /// バグ2の回帰: 修飾なしのタイピング（interrupt）のあと、対象キーの押し下げで Start が
    /// 出る。旧実装ではタイピングが毒を残して None になっていた。
    #[test]
    fn typing_before_a_hold_does_not_swallow_the_first_press() {
        let mut s = HoldState::default();
        assert_eq!(s.on_interrupt(), None, "hold していないときの割込みは何も出さない");
        assert_eq!(press(&mut s), Some(Edge::Start), "タイピング直後の1回目のholdが効く");
    }

    /// バグ1の回帰: hold 中の割込みは End ではなく Cancel。続くリリースは二重に End を出さず、
    /// 完全リリースで再武装したあとは次の hold が始まる。
    #[test]
    fn an_interrupt_during_a_hold_cancels_rather_than_ending() {
        let mut s = HoldState::default();
        assert_eq!(press(&mut s), Some(Edge::Start));
        assert_eq!(s.on_interrupt(), Some(Edge::Cancel), "割込みは送信ではなく破棄");
        assert_eq!(release(&mut s), None, "潰したあとのリリースで End を二重に出さない");
        assert_eq!(press(&mut s), Some(Edge::Start), "完全リリースで再武装済み");
    }

    /// 潰したあと、キーを離さないまま（down のまま）再度 flagsChanged が来ても何も出ない。
    /// poison が効いている。
    #[test]
    fn a_poisoned_hold_stays_dead_until_the_key_is_released() {
        let mut s = HoldState::default();
        press(&mut s);
        assert_eq!(s.on_interrupt(), Some(Edge::Cancel));
        // 対象キーは down のまま。down→down はエッジでないので何も起きないが、was_down が
        // すでに true なので押し下げエッジも来ない。
        assert_eq!(s.on_flags_changed(TC, TD, TC, TD, FOREIGN), None, "poison中は無反応");
    }

    /// 左⌘（foreign）を押したまま対象キーを押しても Start は出ない（和音）。左⌘を離すと
    /// 再武装し、以降の対象キー単独の押し下げで Start。
    #[test]
    fn a_chord_start_is_ignored_and_rearms_when_the_other_key_lifts() {
        let mut s = HoldState::default();
        // 左⌘ down（対象キー以外の flagsChanged、foreign が押されている）。
        assert_eq!(s.on_flags_changed(55, LEFT_CMD, TC, TD, FOREIGN), None);
        // 左⌘を押したまま対象キーの押し下げエッジ。和音なので Start は出ず、poison される。
        assert_eq!(s.on_flags_changed(TC, TD | LEFT_CMD, TC, TD, FOREIGN), None, "和音では始めない");
        // 左⌘を離す（対象キー以外の flagsChanged、foreign が無くなった）→ 再武装。
        assert_eq!(s.on_flags_changed(55, 0, TC, TD, FOREIGN), None);
        // ここで対象キーは down のままなので、いったんリリースしてから単独で押し直す。
        assert_eq!(release(&mut s), None, "潰されたholdのリリースは End を出さない");
        assert_eq!(press(&mut s), Some(Edge::Start), "単独の押し下げは通る");
    }

    /// 正常経路の回帰: hold 中の対象キーのリリースエッジで End が出る。
    #[test]
    fn a_normal_release_ends_the_hold() {
        let mut s = HoldState::default();
        assert_eq!(press(&mut s), Some(Edge::Start));
        assert_eq!(release(&mut s), Some(Edge::End));
    }

    /// hold していないときに対象キー以外の修飾が離れても何も出ず、その後の hold は成功する。
    #[test]
    fn a_foreign_release_while_idle_is_harmless() {
        let mut s = HoldState::default();
        // 対象キー以外の flagsChanged（foreign なし = 何かが離れた）。
        assert_eq!(s.on_flags_changed(55, 0, TC, TD, FOREIGN), None);
        assert_eq!(press(&mut s), Some(Edge::Start), "その後の hold は成功する");
        assert_eq!(release(&mut s), Some(Edge::End));
    }
}
