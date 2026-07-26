//! On-device probe for the meeting lane's native signals.
//!
//! The pure logic is unit-tested off-device; what cannot be is whether macOS actually answers
//! the questions this feature asks it. This binary asks them once a second and prints the
//! answers, so the parts that can only be verified on a real Mac — Accessibility returning a
//! window title, Safari/Chrome exposing the page URL, NSWorkspace reporting a running app — can
//! be checked without launching the app, opening a panel, or writing to the real database.
//!
//! Run it, then bring Zoom or a Google Meet call to the front:
//!
//! ```sh
//! cargo run -p shogun-desktop-spike --example meeting_probe
//! ```

use shogun_core::meeting::detect::{self, Decision, Signals};
use shogun_desktop_spike_lib::{axcache, display};

fn main() {
    if !axcache::ax_trusted() {
        eprintln!("Accessibility permission is not granted for this binary's parent process.");
        eprintln!("Grant it to your terminal in System Settings → Privacy & Security →");
        eprintln!("Accessibility, then run this again.");
        return;
    }
    println!("Accessibility: granted. Probing once a second — ^C to stop.\n");

    let mut last = String::new();
    loop {
        let line = probe();
        // Only print on change: a steady state should be quiet, so anything that scrolls is a
        // transition worth looking at.
        if line != last {
            println!("{line}");
            last = line;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

fn probe() -> String {
    let Some(front) = display::frontmost_app() else {
        return "no frontmost app".to_string();
    };
    let title = axcache::focused_window(front.pid)
        .and_then(|w| w.title())
        .unwrap_or_else(|| "(no title)".into());
    let url = axcache::browser_url(front.pid);

    let signals = Signals {
        meeting_app_frontmost: detect::is_meeting_app(&front.bundle_id)
            || url.as_deref().is_some_and(detect::is_meeting_url),
        ..Default::default()
    };
    let verdict = match detect::decide(&signals) {
        Decision::Offer { confidence, .. } => format!("OFFER (confidence {confidence:.2})"),
        Decision::Ignore => "ignore".to_string(),
    };

    format!(
        "{:<34} running={:<5} title={:<44} url={:<52} → {verdict}",
        front.bundle_id,
        display::is_app_running(&front.bundle_id),
        truncate(&title, 44),
        truncate(url.as_deref().unwrap_or("-"), 52),
    )
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).collect::<String>() + "…"
    }
}
