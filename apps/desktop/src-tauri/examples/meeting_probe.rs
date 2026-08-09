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

    // Fixed duration rather than run-until-interrupted: the thing being measured is what happens
    // in *other* apps, so the operator has to leave this window. Requiring them to come back and
    // press ^C is how a probe ends up recording nothing but the terminal it was started from.
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(45);

    println!("=========================================================");
    println!("  {seconds} 秒間記録します。**今すぐ**他のアプリに切り替えてください。");
    println!();
    println!("   1. ブラウザ（できれば Google Meet の会議中）に切り替えて数秒");
    println!("   2. Zoom があれば Zoom に切り替えて数秒");
    println!("   3. あとは放っておけば自動で終わります（^C 不要）");
    println!("=========================================================");
    println!();

    let mut seen: Vec<String> = Vec::new();
    let mut last = String::new();
    for _ in 0..seconds {
        let (key, line) = probe_pair();
        if line != last {
            println!("{line}");
            last = line;
        }
        if !seen.contains(&key) {
            seen.push(key);
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    println!();
    println!("=================== 結果 ===================");
    for row in &seen {
        println!("{row}");
    }
    if seen.len() <= 1 {
        println!();
        println!("⚠ 前面アプリが1つしか観測されていません。記録中に他のアプリへ");
        println!("  切り替えられていない可能性があります。もう一度お試しください。");
    }
    println!("============================================");
}

/// One observation, as (summary-for-the-report, detail-line).
fn probe_pair() -> (String, String) {
    let line = probe();
    let key = match display::frontmost_app() {
        Some(f) => {
            let url = axcache::browser_url(f.pid);
            format!(
                "{:<34} url={}",
                f.bundle_id,
                match url {
                    Some(u) => truncate(&u, 60),
                    None => "取得できず(-)".to_string(),
                }
            )
        }
        None => "no frontmost app".to_string(),
    };
    (key, line)
}

fn probe() -> String {
    let Some(front) = display::frontmost_app() else {
        return "no frontmost app".to_string();
    };
    let title = axcache::focused_window(front.pid)
        .and_then(|w| w.title())
        .unwrap_or_else(|| "(no title)".into());
    let url = axcache::browser_url(front.pid);

    // Strong surfaces only, mirroring the adapter: Weak (Teams, Webex) corroborates via ctx.
    let signals = Signals {
        meeting_app_frontmost: detect::bundle_hint(&front.bundle_id)
            == Some(detect::MeetingHint::Strong)
            || url.as_deref().and_then(detect::host_hint) == Some(detect::MeetingHint::Strong),
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
