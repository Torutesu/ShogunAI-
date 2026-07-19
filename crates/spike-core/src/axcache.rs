//! Context-cache walk policy and in-memory cache (spec §3.10.2).
//!
//! The *policy* — BFS with depth ≤8, ≤300 elements, ≤32KB, role filtering, and total
//! skip of AXSecureTextField subtrees — is pure and tested here over an abstract [`AxNode`]
//! tree. The macOS layer implements [`AxNode`] via AXUIElement (with a 100ms messaging
//! timeout and a 250ms overall timebox expressed through `should_stop`), keeping every AX
//! call confined to that adapter (the "no collect-on-press" invariant, spec §3.10.3).

/// Accessibility roles relevant to text collection (spec §3.10.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    StaticText,
    TextArea,
    TextField,
    Heading,
    Link,
    Cell,
    /// Never read; the whole subtree is skipped (password fields).
    SecureTextField,
    /// Any other role — traversed for children but not itself collected.
    Other,
}

impl Role {
    fn is_collectable(self) -> bool {
        matches!(
            self,
            Role::StaticText | Role::TextArea | Role::TextField | Role::Heading | Role::Link | Role::Cell
        )
    }
}

/// Walk budgets (spec §3.10.2). Overridable for the Q3 retry loop (depth 8→6, 300→200).
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_depth: u32,
    pub max_elements: u32,
    pub max_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_depth: 8, max_elements: 300, max_bytes: 32 * 1024 }
    }
}

/// An abstract accessibility element. The macOS adapter implements this over AXUIElement;
/// tests implement it over a plain struct. `children` is only called within the depth
/// budget, so the adapter never over-fetches.
pub trait AxNode: Sized {
    fn role(&self) -> Role;
    /// Resolved text: value → title → description (adapter applies that order).
    fn value_text(&self) -> Option<String>;
    fn children(&self) -> Vec<Self>;
}

/// Outcome of one walk.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct WalkResult {
    pub text: String,
    pub text_bytes: usize,
    pub elements_visited: u32,
    pub depth_reached: u32,
    pub truncated: bool,
    /// True if `should_stop` fired (generation cancelled or 250ms timebox hit).
    pub partial: bool,
}

/// Walk `root` under `limits`, calling `should_stop` before each element so the adapter
/// can enforce cancellation and the time budget. BFS; SecureTextField subtrees are skipped.
///
/// `N: Clone` because the macOS AXUIElement wrapper is a cheap CF retain; the queue owns
/// nodes so it can be built incrementally within the depth budget.
pub fn walk<N: AxNode + Clone>(root: &N, limits: Limits, mut should_stop: impl FnMut() -> bool) -> WalkResult {
    let mut res = WalkResult::default();
    // Queue of (node, depth). Depth 0 = root.
    let mut queue: std::collections::VecDeque<(N, u32)> = std::collections::VecDeque::new();
    queue.push_back((root.clone(), 0));

    while let Some((node, depth)) = queue.pop_front() {
        if should_stop() {
            res.partial = true;
            break;
        }
        if res.elements_visited >= limits.max_elements {
            res.truncated = true;
            break;
        }
        let role = node.role();
        if role == Role::SecureTextField {
            // Skip the node and its entire subtree.
            continue;
        }
        res.elements_visited += 1;
        res.depth_reached = res.depth_reached.max(depth);

        if role.is_collectable() {
            if let Some(t) = node.value_text() {
                let t = t.trim();
                if !t.is_empty() {
                    if res.text_bytes + t.len() + 1 > limits.max_bytes {
                        res.truncated = true;
                        break;
                    }
                    if !res.text.is_empty() {
                        res.text.push('\n');
                        res.text_bytes += 1;
                    }
                    res.text.push_str(t);
                    res.text_bytes += t.len();
                }
            }
        }

        if depth < limits.max_depth {
            for child in node.children() {
                queue.push_back((child, depth + 1));
            }
        }
    }
    res
}

/// The in-memory current context (spec §3.10.2 step 6). Holds text for on-screen display;
/// this is the live cache, not a log — the privacy rule (no text in sinks) is enforced at
/// the harness/record boundary, not here.
#[derive(Clone, Debug, Default)]
pub struct ContextCache {
    pub gen: u64,
    pub bundle_id: String,
    pub pid: i32,
    pub window_title: String,
    pub text: String,
    pub text_bytes: usize,
    pub captured_at_ms: u64,
    pub duration_ms: u64,
    pub partial: bool,
    pub truncated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Fake {
        role: Role,
        text: Option<&'static str>,
        children: Vec<Fake>,
    }
    impl Fake {
        fn leaf(role: Role, text: &'static str) -> Fake {
            Fake { role, text: Some(text), children: vec![] }
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
            self.text.map(|s| s.to_string())
        }
        fn children(&self) -> Vec<Self> {
            self.children.clone()
        }
    }

    fn never_stop() -> bool {
        false
    }

    #[test]
    fn collects_nested_text_in_order() {
        let tree = Fake::group(vec![
            Fake::leaf(Role::Heading, "Title"),
            Fake::group(vec![Fake::leaf(Role::StaticText, "body one"), Fake::leaf(Role::Link, "link")]),
        ]);
        let r = walk(&tree, Limits::default(), never_stop);
        assert_eq!(r.text, "Title\nbody one\nlink");
        assert_eq!(r.text_bytes, r.text.len());
        assert!(!r.truncated && !r.partial);
    }

    #[test]
    fn secure_field_subtree_is_skipped() {
        let tree = Fake::group(vec![
            Fake::leaf(Role::StaticText, "visible"),
            Fake {
                role: Role::SecureTextField,
                text: Some("hunter2"),
                children: vec![Fake::leaf(Role::StaticText, "child of secret")],
            },
        ]);
        let r = walk(&tree, Limits::default(), never_stop);
        assert!(r.text.contains("visible"));
        assert!(!r.text.contains("hunter2"));
        assert!(!r.text.contains("child of secret"));
    }

    #[test]
    fn depth_limit_stops_descent() {
        // Chain of groups 12 deep with a text leaf at the bottom.
        let mut node = Fake::leaf(Role::StaticText, "deep");
        for _ in 0..12 {
            node = Fake::group(vec![node]);
        }
        let limits = Limits { max_depth: 3, ..Limits::default() };
        let r = walk(&node, limits, never_stop);
        assert!(r.depth_reached <= 3);
        assert!(!r.text.contains("deep"));
    }

    #[test]
    fn element_budget_truncates() {
        let kids: Vec<Fake> = (0..500).map(|_| Fake::leaf(Role::StaticText, "x")).collect();
        let tree = Fake::group(kids);
        let limits = Limits { max_elements: 50, ..Limits::default() };
        let r = walk(&tree, limits, never_stop);
        assert!(r.truncated);
        assert!(r.elements_visited <= 50);
    }

    #[test]
    fn byte_budget_truncates() {
        let big = "0123456789".repeat(10); // 100 bytes
        let leaked: &'static str = Box::leak(big.into_boxed_str());
        let kids: Vec<Fake> = (0..100).map(|_| Fake::leaf(Role::StaticText, leaked)).collect();
        let tree = Fake::group(kids);
        let limits = Limits { max_bytes: 250, ..Limits::default() };
        let r = walk(&tree, limits, never_stop);
        assert!(r.truncated);
        assert!(r.text_bytes <= 250);
    }

    #[test]
    fn cancellation_marks_partial() {
        let tree = Fake::group((0..100).map(|_| Fake::leaf(Role::StaticText, "x")).collect());
        let mut n = 0;
        let r = walk(&tree, Limits::default(), || {
            n += 1;
            n > 5 // stop after a few elements
        });
        assert!(r.partial);
    }

    #[test]
    fn non_collectable_roles_are_not_captured() {
        let tree = Fake::group(vec![
            Fake::leaf(Role::Other, "chrome"),
            Fake::leaf(Role::StaticText, "content"),
        ]);
        let r = walk(&tree, Limits::default(), never_stop);
        assert_eq!(r.text, "content");
    }
}
