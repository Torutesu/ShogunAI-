//! Push-to-talk 音声対話（Issue #44）: ショートカットを長押ししている間だけマイクを開き、
//! 離した瞬間に文字起こし → コンテキスト結合 → エージェント応答までを一息で走らせる入口。
//!
//! ここにあるのは全て純ロジックで、マイクにもネットワークにも触らない。実際の副作用は
//! `apps/desktop/src-tauri/src/ptt.rs` の実行層が [`statemachine::Effect`] を解釈して行う。
//! 不変条件2の担保もここが要: 波形は `shogun_core::audio` のRAMバッファにしか存在せず、
//! BufferSink は文字起こしテキストだけを受けてDBにも書かない。

pub mod statemachine;
