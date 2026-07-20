//! Notch state machine (spec §6.1.1 / FR-NU-01) — the single owner of UI state.
//!
//! Deterministic and time-injected: it consumes [`Input`]s (hover signals, timer expiries,
//! key/click/hotkey/fullscreen events) and returns [`Effect`]s (state transitions, timer
//! start/cancel, mouse-passthrough toggles, the two open-latency commit markers, and the
//! "open Full UI" request). It owns no real timers or threads — the macOS adapter schedules
//! timers and feeds the expiries back in. Every transition is unit-testable off-device.
//!
//! ## Two-level open model (the key change from the Phase 0 spike)
//!
//! The spike used a single open level (dwell → one panel). FR-NU-01 specifies **two**:
//!
//! ```text
//!   Idle ──hover 120ms──▶ Hover(preview) ──click / ⌘⇧Space──▶ Expanded(full)
//!        ◀─mouse-leave 200ms─┘          ◀─Esc / outside / 20s idle─┘
//!   Idle ──⌘⇧Space────────────────────────────────────────────▶ Expanded  (direct)
//!   any  ──app went fullscreen──▶ Hidden ──fullscreen ended──▶ Idle
//! ```
//!
//! - **Hover** is the lightweight preview (1 action + 1 status line, FR-NU-01).
//! - **Expanded** is the full panel (≤4 actions + Morning Brief + chat, FR-NU-01).
//! - `HoverIntent` is an internal dwell-pending state (visually still Idle) so the dwell can
//!   be cancelled on early exit; it is not one of the spec's named UI states.
//! - `Collapsing` is the animation-out intermediate (retained from the spike, with its T5
//!   revive) so the adapter knows when the close animation has finished.

/// Timer kinds the adapter must schedule when asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timer {
    /// Idle→Hover dwell (default 120ms, or 250ms on a fast approach). FR-NU-01.
    Dwell,
    /// Hover(preview) mouse-leave grace (200ms) before it collapses. FR-NU-01.
    HoverExit,
    /// Expanded(full) no-interaction timeout (20s) before it collapses. FR-NU-01.
    ExpandedIdle,
    /// Collapse-animation timeout (400ms; the webview normally reports AnimDone at ~160ms).
    CollapseAnim,
}

/// The UI states. `Idle`/`Hover`/`Expanded`/`Hidden` are the spec's named states; `HoverIntent`
/// (dwell pending) and `Collapsing` (animating out) are internal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    HoverIntent,
    Hover,
    Expanded,
    Collapsing,
    Hidden,
}

impl State {
    pub fn tag(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::HoverIntent => "hoverintent",
            State::Hover => "hover",
            State::Expanded => "expanded",
            State::Collapsing => "collapsing",
            State::Hidden => "hidden",
        }
    }
}

/// Inputs into the machine. Hover signals arrive from [`crate::notch::hover`]; timer expiries,
/// key/click/hotkey, Full-UI, and fullscreen events arrive from the macOS adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// Entered R_enter under hover conditions. `fast` selects the longer dwell.
    HoverEnter { fast: bool },
    /// Left the stay region (cancels a pending dwell; arms the Hover-exit grace).
    HoverExitStay,
    /// Re-entered the stay region (disarms the Hover-exit grace).
    HoverReenter,
    /// Dwell timer fired → show the preview.
    DwellExpired,
    /// A mouse button went down during the dwell (menubar intent) — cancel.
    ButtonDown,
    /// Hover-exit grace fired → collapse the preview.
    HoverExitExpired,
    /// Click on the preview/panel → open the full panel.
    Click,
    /// Global hotkey (⌘⇧Space) → open the full panel (from Idle/Hover/Hidden).
    Hotkey,
    /// Any interaction inside Expanded — resets the 20s idle timeout.
    Interaction,
    /// Expanded idle timeout fired (20s no interaction).
    ExpandedIdleExpired,
    /// Esc while the panel is key.
    Esc,
    /// Click in the transparent margin (outside the visible panel).
    OutsideClick,
    /// "Open Full UI" chosen from Expanded.
    OpenFullUi,
    /// Re-entered R_enter during Collapsing (revive → Hover preview).
    ReenterEnter,
    /// Collapse animation reported done by the webview.
    AnimDone,
    /// Collapse animation timeout — webview hang suspicion.
    AnimTimeout,
    /// Focused app entered fullscreen → hide (FR-NU-08).
    EnterFullscreen,
    /// Focused app left fullscreen → return to Idle.
    ExitFullscreen,
    /// Display change / sleep — force collapse.
    ForceCollapse,
}

/// Side effects the adapter must apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Transition(State),
    StartTimer { timer: Timer, ms: u64 },
    CancelTimer(Timer),
    /// Toggle `ignoresMouseEvents` (spec §3.1.3): false in the interactive states
    /// (Hover/Expanded), true everywhere else.
    SetIgnoresMouse(bool),
    /// `t0` for the preview-open latency (Idle→Hover) — the Phase 0 Q2 measurement.
    MarkPreviewCommit,
    /// `t0` for the full-expand latency (→Expanded), i.e. NFR-SLO-01.
    MarkExpandCommit,
    /// Request the adapter open the separate Full UI window.
    OpenFullUi,
}

/// Tunable timers (spec FR-NU-01 / Appendix A). Dwell is adjustable in 60–200ms per FR-NU-01.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub dwell_ms: u64,
    pub dwell_fast_ms: u64,
    pub hover_exit_ms: u64,
    pub expanded_idle_ms: u64,
    pub collapse_timeout_ms: u64,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            dwell_ms: 120,
            dwell_fast_ms: 250,
            hover_exit_ms: 200,
            expanded_idle_ms: 20_000,
            collapse_timeout_ms: 400,
        }
    }
}

/// The state machine.
#[derive(Debug)]
pub struct StateMachine {
    state: State,
    params: Params,
}

impl StateMachine {
    pub fn new(params: Params) -> Self {
        Self { state: State::Idle, params }
    }

    pub fn state(&self) -> State {
        self.state
    }

    fn go(&mut self, next: State, fx: &mut Vec<Effect>) {
        self.state = next;
        fx.push(Effect::Transition(next));
    }

    /// Enter Expanded (full) from `Idle`/`HoverIntent`/`Hover`/`Hidden`: mark the SLO-01 commit,
    /// make the panel interactive, and arm the 20s idle timeout. `from_preview` skips the
    /// passthrough toggle (Hover is already interactive).
    fn open_expanded(&mut self, from_preview: bool, fx: &mut Vec<Effect>) {
        self.go(State::Expanded, fx);
        fx.push(Effect::MarkExpandCommit);
        if !from_preview {
            fx.push(Effect::SetIgnoresMouse(false));
        }
        fx.push(Effect::StartTimer { timer: Timer::ExpandedIdle, ms: self.params.expanded_idle_ms });
    }

    /// Begin the collapse animation from any open state. Cancels the state's live timer,
    /// restores passthrough, and arms the animation timeout.
    fn begin_collapse(&mut self, cancel: Option<Timer>, fx: &mut Vec<Effect>) {
        if let Some(t) = cancel {
            fx.push(Effect::CancelTimer(t));
        }
        self.go(State::Collapsing, fx);
        fx.push(Effect::SetIgnoresMouse(true));
        fx.push(Effect::StartTimer { timer: Timer::CollapseAnim, ms: self.params.collapse_timeout_ms });
    }

    /// Apply one input, returning the effects to enact. Unhandled `(state, input)` pairs are
    /// deliberate no-ops (empty vec) — the machine never panics on an unexpected event.
    pub fn step(&mut self, input: Input) -> Vec<Effect> {
        use Input::*;
        use State::*;
        let mut fx = Vec::new();
        match (self.state, input) {
            // ---- global: fullscreen hides from any non-hidden state (FR-NU-08) ----
            (Idle, EnterFullscreen) => self.go(Hidden, &mut fx),
            (HoverIntent, EnterFullscreen) => {
                fx.push(Effect::CancelTimer(Timer::Dwell));
                self.go(Hidden, &mut fx);
            }
            (Hover, EnterFullscreen) => {
                fx.push(Effect::CancelTimer(Timer::HoverExit));
                fx.push(Effect::SetIgnoresMouse(true));
                self.go(Hidden, &mut fx);
            }
            (Expanded, EnterFullscreen) => {
                fx.push(Effect::CancelTimer(Timer::ExpandedIdle));
                fx.push(Effect::SetIgnoresMouse(true));
                self.go(Hidden, &mut fx);
            }
            (Collapsing, EnterFullscreen) => {
                fx.push(Effect::CancelTimer(Timer::CollapseAnim));
                self.go(Hidden, &mut fx);
            }

            // ---- Idle ----
            (Idle, HoverEnter { fast }) => {
                let ms = if fast { self.params.dwell_fast_ms } else { self.params.dwell_ms };
                self.go(HoverIntent, &mut fx);
                fx.push(Effect::StartTimer { timer: Timer::Dwell, ms });
            }
            (Idle, Hotkey) => self.open_expanded(false, &mut fx),

            // ---- HoverIntent (dwell pending) ----
            (HoverIntent, DwellExpired) => {
                self.go(Hover, &mut fx);
                fx.push(Effect::MarkPreviewCommit);
                fx.push(Effect::SetIgnoresMouse(false));
            }
            (HoverIntent, HoverExitStay | ButtonDown | ForceCollapse) => {
                fx.push(Effect::CancelTimer(Timer::Dwell));
                self.go(Idle, &mut fx);
            }
            (HoverIntent, Hotkey) => {
                fx.push(Effect::CancelTimer(Timer::Dwell));
                self.open_expanded(false, &mut fx);
            }

            // ---- Hover (preview) ----
            (Hover, HoverExitStay) => {
                fx.push(Effect::StartTimer { timer: Timer::HoverExit, ms: self.params.hover_exit_ms });
            }
            (Hover, HoverReenter) => fx.push(Effect::CancelTimer(Timer::HoverExit)),
            (Hover, Click | Hotkey) => {
                fx.push(Effect::CancelTimer(Timer::HoverExit));
                self.open_expanded(true, &mut fx);
            }
            (Hover, HoverExitExpired | Esc | ForceCollapse) => {
                self.begin_collapse(Some(Timer::HoverExit), &mut fx);
            }

            // ---- Expanded (full) ----
            (Expanded, Interaction) => {
                fx.push(Effect::CancelTimer(Timer::ExpandedIdle));
                fx.push(Effect::StartTimer { timer: Timer::ExpandedIdle, ms: self.params.expanded_idle_ms });
            }
            (Expanded, ExpandedIdleExpired | Esc | OutsideClick | ForceCollapse) => {
                self.begin_collapse(Some(Timer::ExpandedIdle), &mut fx);
            }
            (Expanded, OpenFullUi) => {
                fx.push(Effect::OpenFullUi);
                self.begin_collapse(Some(Timer::ExpandedIdle), &mut fx);
            }

            // ---- Collapsing ----
            (Collapsing, ReenterEnter) => {
                fx.push(Effect::CancelTimer(Timer::CollapseAnim));
                self.go(Hover, &mut fx);
                fx.push(Effect::SetIgnoresMouse(false));
            }
            (Collapsing, AnimDone | AnimTimeout) => self.go(Idle, &mut fx),

            // ---- Hidden (fullscreen) ----
            (Hidden, ExitFullscreen) => self.go(Idle, &mut fx),
            // FR-NU-08: an explicit hotkey call may show Expanded over fullscreen.
            (Hidden, Hotkey) => self.open_expanded(false, &mut fx),

            // Everything else is a deliberate no-op.
            _ => {}
        }
        fx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sm() -> StateMachine {
        StateMachine::new(Params::default())
    }

    /// Drive Idle → Hover(preview) via the dwell.
    fn to_hover(m: &mut StateMachine) {
        m.step(Input::HoverEnter { fast: false });
        m.step(Input::DwellExpired);
        assert_eq!(m.state(), State::Hover);
    }

    /// Drive Idle → Hover → Expanded via a click.
    fn to_expanded(m: &mut StateMachine) {
        to_hover(m);
        m.step(Input::Click);
        assert_eq!(m.state(), State::Expanded);
    }

    #[test]
    fn dwell_opens_preview_not_full_panel() {
        let mut m = sm();
        let fx = m.step(Input::HoverEnter { fast: false });
        assert_eq!(m.state(), State::HoverIntent);
        assert!(fx.contains(&Effect::StartTimer { timer: Timer::Dwell, ms: 120 }));

        let fx = m.step(Input::DwellExpired);
        assert_eq!(m.state(), State::Hover);
        assert!(fx.contains(&Effect::MarkPreviewCommit));
        assert!(fx.contains(&Effect::SetIgnoresMouse(false)));
        // Dwell must NOT jump straight to the full panel.
        assert!(!fx.contains(&Effect::MarkExpandCommit));
    }

    #[test]
    fn fast_entry_uses_longer_dwell() {
        let mut m = sm();
        let fx = m.step(Input::HoverEnter { fast: true });
        assert!(fx.contains(&Effect::StartTimer { timer: Timer::Dwell, ms: 250 }));
    }

    #[test]
    fn exit_during_dwell_cancels_and_returns_idle() {
        let mut m = sm();
        m.step(Input::HoverEnter { fast: false });
        let fx = m.step(Input::HoverExitStay);
        assert_eq!(m.state(), State::Idle);
        assert!(fx.contains(&Effect::CancelTimer(Timer::Dwell)));
    }

    #[test]
    fn click_opens_full_panel_from_preview() {
        let mut m = sm();
        to_hover(&mut m);
        let fx = m.step(Input::Click);
        assert_eq!(m.state(), State::Expanded);
        assert!(fx.contains(&Effect::MarkExpandCommit));
        assert!(fx.contains(&Effect::CancelTimer(Timer::HoverExit)));
        assert!(fx.contains(&Effect::StartTimer { timer: Timer::ExpandedIdle, ms: 20_000 }));
        // Already interactive as a preview — no redundant passthrough toggle.
        assert!(!fx.contains(&Effect::SetIgnoresMouse(false)));
    }

    #[test]
    fn hotkey_opens_full_panel_direct_from_idle() {
        let mut m = sm();
        let fx = m.step(Input::Hotkey);
        assert_eq!(m.state(), State::Expanded);
        assert!(fx.contains(&Effect::MarkExpandCommit));
        assert!(fx.contains(&Effect::SetIgnoresMouse(false)));
        assert!(fx.contains(&Effect::StartTimer { timer: Timer::ExpandedIdle, ms: 20_000 }));
    }

    #[test]
    fn hotkey_during_dwell_jumps_to_full() {
        let mut m = sm();
        m.step(Input::HoverEnter { fast: false });
        let fx = m.step(Input::Hotkey);
        assert_eq!(m.state(), State::Expanded);
        assert!(fx.contains(&Effect::CancelTimer(Timer::Dwell)));
        assert!(fx.contains(&Effect::MarkExpandCommit));
    }

    #[test]
    fn preview_leave_grace_collapses_then_reenter_disarms() {
        let mut m = sm();
        to_hover(&mut m);
        let fx = m.step(Input::HoverExitStay);
        assert!(fx.contains(&Effect::StartTimer { timer: Timer::HoverExit, ms: 200 }));
        assert_eq!(m.state(), State::Hover);
        // Re-enter cancels the grace.
        let fx = m.step(Input::HoverReenter);
        assert!(fx.contains(&Effect::CancelTimer(Timer::HoverExit)));
        assert_eq!(m.state(), State::Hover);
        // Grace expiry collapses the preview.
        m.step(Input::HoverExitStay);
        let fx = m.step(Input::HoverExitExpired);
        assert_eq!(m.state(), State::Collapsing);
        assert!(fx.contains(&Effect::SetIgnoresMouse(true)));
        assert!(fx.contains(&Effect::StartTimer { timer: Timer::CollapseAnim, ms: 400 }));
    }

    #[test]
    fn expanded_interaction_resets_idle_timeout() {
        let mut m = sm();
        to_expanded(&mut m);
        let fx = m.step(Input::Interaction);
        assert!(fx.contains(&Effect::CancelTimer(Timer::ExpandedIdle)));
        assert!(fx.contains(&Effect::StartTimer { timer: Timer::ExpandedIdle, ms: 20_000 }));
        assert_eq!(m.state(), State::Expanded);
    }

    #[test]
    fn expanded_closes_on_esc_outside_and_idle_timeout() {
        for close in [Input::Esc, Input::OutsideClick, Input::ExpandedIdleExpired] {
            let mut m = sm();
            to_expanded(&mut m);
            let fx = m.step(close);
            assert_eq!(m.state(), State::Collapsing, "input {close:?}");
            assert!(fx.contains(&Effect::CancelTimer(Timer::ExpandedIdle)));
            assert!(fx.contains(&Effect::SetIgnoresMouse(true)));
        }
    }

    #[test]
    fn open_full_ui_emits_effect_and_collapses() {
        let mut m = sm();
        to_expanded(&mut m);
        let fx = m.step(Input::OpenFullUi);
        assert!(fx.contains(&Effect::OpenFullUi));
        assert_eq!(m.state(), State::Collapsing);
    }

    #[test]
    fn reenter_during_collapsing_revives_to_preview() {
        let mut m = sm();
        to_hover(&mut m);
        m.step(Input::HoverExitStay);
        m.step(Input::HoverExitExpired);
        assert_eq!(m.state(), State::Collapsing);
        let fx = m.step(Input::ReenterEnter);
        assert_eq!(m.state(), State::Hover);
        assert!(fx.contains(&Effect::CancelTimer(Timer::CollapseAnim)));
        assert!(fx.contains(&Effect::SetIgnoresMouse(false)));
    }

    #[test]
    fn collapsing_finishes_to_idle() {
        for done in [Input::AnimDone, Input::AnimTimeout] {
            let mut m = sm();
            to_expanded(&mut m);
            m.step(Input::Esc);
            let _ = m.step(done);
            assert_eq!(m.state(), State::Idle, "input {done:?}");
        }
    }

    #[test]
    fn fullscreen_hides_from_every_open_state_and_restores() {
        // From Hover.
        let mut m = sm();
        to_hover(&mut m);
        let fx = m.step(Input::EnterFullscreen);
        assert_eq!(m.state(), State::Hidden);
        assert!(fx.contains(&Effect::CancelTimer(Timer::HoverExit)));
        assert!(fx.contains(&Effect::SetIgnoresMouse(true)));
        assert_eq!(m.step(Input::ExitFullscreen), vec![Effect::Transition(State::Idle)]);

        // From Expanded.
        let mut m = sm();
        to_expanded(&mut m);
        let fx = m.step(Input::EnterFullscreen);
        assert_eq!(m.state(), State::Hidden);
        assert!(fx.contains(&Effect::CancelTimer(Timer::ExpandedIdle)));
    }

    #[test]
    fn hotkey_shows_expanded_over_fullscreen() {
        let mut m = sm();
        m.step(Input::EnterFullscreen);
        assert_eq!(m.state(), State::Hidden);
        let fx = m.step(Input::Hotkey);
        assert_eq!(m.state(), State::Expanded);
        assert!(fx.contains(&Effect::MarkExpandCommit));
    }

    #[test]
    fn unexpected_input_is_noop() {
        let mut m = sm();
        // Click in Idle does nothing (no preview to promote).
        assert!(m.step(Input::Click).is_empty());
        assert_eq!(m.state(), State::Idle);
        // DwellExpired in Idle does nothing.
        assert!(m.step(Input::DwellExpired).is_empty());
    }
}
