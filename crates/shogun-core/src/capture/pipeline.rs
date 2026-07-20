//! The capture pipeline (WP2.2, §6.2) — platform-independent composition of the capture policy.
//!
//! One focus change flows through: **exclusion gate** (privacy, FR-CAP-05/06) → **bounded AX walk**
//! (FR-CAP-01/02, timeboxed) → text. This module owns that composition over the abstract
//! [`AxNode`], so it is fully Linux-testable with a fake tree; the macOS adapter only has to supply
//! the focus stream (AXObserver) and an `AxNode` over `AXUIElement`, then persist a `Captured`
//! outcome through the daemon. No screenshot, no image — AX text only (invariant 2).

use super::exclusion::{ExclusionPolicy, ExclusionReason};
use super::walk_policy::{walk, AxNode, Limits, WalkResult};

/// The frontmost app/window a capture evaluates.
#[derive(Debug, Clone, Copy)]
pub struct Focus<'a> {
    pub bundle_id: &'a str,
    pub window_title: Option<&'a str>,
}

/// What processing one focus produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureOutcome {
    /// The focus was excluded by the privacy gate — nothing was read (FR-CAP-05/06).
    Excluded(ExclusionReason),
    /// The AX walk yielded no usable text (empty window, or all elements filtered).
    Empty,
    /// Text was captured; `walk` carries the walk telemetry (bytes, truncation, partial).
    Captured { text: String, walk: WalkResult },
}

impl CaptureOutcome {
    /// The captured text, if any (convenience for the persist step).
    pub fn text(&self) -> Option<&str> {
        match self {
            CaptureOutcome::Captured { text, .. } => Some(text),
            _ => None,
        }
    }
}

/// Run the capture policy for one focus over an AX `root`, **without touching a DB** — pure over the
/// exclusion policy + the AX walk. The exclusion gate runs *first*, so an excluded window's tree is
/// never walked (FR-CAP-05: no read at all). `should_stop` enforces the adapter's cancellation and
/// 250 ms time budget (FR-CAP-02).
pub fn capture_focus<N: AxNode + Clone>(
    policy: &ExclusionPolicy,
    focus: &Focus<'_>,
    root: &N,
    limits: Limits,
    should_stop: impl FnMut() -> bool,
) -> CaptureOutcome {
    if let Some(reason) = policy.is_excluded(focus.bundle_id, focus.window_title) {
        return CaptureOutcome::Excluded(reason);
    }
    let result = walk(root, limits, should_stop);
    if result.text.trim().is_empty() {
        return CaptureOutcome::Empty;
    }
    CaptureOutcome::Captured { text: result.text.clone(), walk: result }
}

#[cfg(test)]
mod tests {
    use super::super::walk_policy::Role;
    use super::*;

    #[derive(Clone)]
    struct Fake {
        role: Role,
        text: Option<String>,
        children: Vec<Fake>,
    }
    impl Fake {
        fn leaf(role: Role, text: &str) -> Fake {
            Fake { role, text: Some(text.to_string()), children: vec![] }
        }
        fn group(children: Vec<Fake>) -> Fake {
            Fake { role: Role::Other, text: None, children }
        }
    }
    impl AxNode for Fake {
        fn role(&self) -> Role {
            self.role
        }
        fn value_text(&self) -> Option<String> {
            self.text.clone()
        }
        fn children(&self) -> Vec<Self> {
            self.children.clone()
        }
    }

    fn tree() -> Fake {
        Fake::group(vec![
            Fake::leaf(Role::StaticText, "the quarterly review"),
            Fake::leaf(Role::StaticText, "with Alice"),
        ])
    }

    #[test]
    fn excluded_focus_is_never_walked() {
        let policy = ExclusionPolicy::new();
        // 1Password is a non-removable default exclusion (FR-CAP-06)
        let focus = Focus { bundle_id: "com.1password.1password", window_title: Some("Vault") };
        // a root that would panic if walked — proving exclusion short-circuits before the walk
        #[derive(Clone)]
        struct Boom;
        impl AxNode for Boom {
            fn role(&self) -> Role {
                panic!("excluded focus must not be walked");
            }
            fn value_text(&self) -> Option<String> {
                panic!("excluded focus must not be walked");
            }
            fn children(&self) -> Vec<Self> {
                panic!("excluded focus must not be walked");
            }
        }
        let out = capture_focus(&policy, &focus, &Boom, Limits::default(), || false);
        assert!(matches!(out, CaptureOutcome::Excluded(_)));
    }

    #[test]
    fn normal_focus_captures_walked_text() {
        let policy = ExclusionPolicy::new();
        let focus = Focus { bundle_id: "com.apple.Safari", window_title: Some("Docs") };
        let out = capture_focus(&policy, &focus, &tree(), Limits::default(), || false);
        match out {
            CaptureOutcome::Captured { text, walk } => {
                assert!(text.contains("quarterly review"));
                assert!(walk.elements_visited > 0);
            }
            other => panic!("expected Captured, got {other:?}"),
        }
    }

    #[test]
    fn empty_tree_is_empty_outcome() {
        let policy = ExclusionPolicy::new();
        let focus = Focus { bundle_id: "com.apple.Safari", window_title: None };
        let empty = Fake::group(vec![]);
        let out = capture_focus(&policy, &focus, &empty, Limits::default(), || false);
        assert_eq!(out, CaptureOutcome::Empty);
    }

    #[test]
    fn cancellation_short_circuits_the_walk() {
        let policy = ExclusionPolicy::new();
        let focus = Focus { bundle_id: "com.apple.Safari", window_title: None };
        // should_stop true immediately → partial walk, no text → Empty
        let out = capture_focus(&policy, &focus, &tree(), Limits::default(), || true);
        assert_eq!(out, CaptureOutcome::Empty);
    }
}
