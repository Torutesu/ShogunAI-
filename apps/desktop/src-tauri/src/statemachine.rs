//! State machine adapter (spec §3.3).
//!
//! The state machine itself lives in `spike_core::statemachine` (pure, unit-tested on
//! Linux). This module is the on-device adapter: it owns a `StateMachine`, schedules the
//! `Timer`s it asks for (tokio/dispatch), and applies `Effect`s — `SetIgnoresMouse` via
//! `panel`, `Transition` via `ipc` (the webview `state` event), and `MarkExpandCommit` as
//! the Q2 `t0` into the harness. Inputs come from `hover` (HoverSignal→Input) and from
//! timer expiries. on-device (T-08).
#![allow(dead_code)]

pub use spike_core::statemachine::{Effect, Input, Params, State, StateMachine, Timer};
