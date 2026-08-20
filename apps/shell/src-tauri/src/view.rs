//! View assembled in Rust (invariant 1). The webview draws this and does not invent health numbers.

use serde::Serialize;
use shogun_platform::{app_data_dir, secret_store_status};

#[derive(Serialize)]
pub struct EmptyPane {
    pub body: String,
}

#[derive(Serialize)]
pub struct ShellView {
    pub os: &'static str,
    pub app_data_dir: String,
    pub secrets_backend: &'static str,
    pub secrets_ready: bool,
    pub secrets_detail: &'static str,
    pub close_behavior: &'static str,
    pub today: EmptyPane,
    pub health: EmptyPane,
    pub sources: EmptyPane,
    pub memory: EmptyPane,
    pub activity: EmptyPane,
    pub trace: EmptyPane,
}

pub fn assemble() -> ShellView {
    let secrets = secret_store_status();
    let dir = app_data_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "unavailable on this OS".to_string());

    ShellView {
        os: current_os(),
        app_data_dir: dir,
        secrets_backend: secrets.backend,
        secrets_ready: secrets.ready,
        secrets_detail: secrets.detail,
        close_behavior: "Closing the window keeps SHOGUN in the tray. Quit from the tray to exit.",
        today: EmptyPane {
            body: "The morning brief is produced by capture and the nightly review in the macOS app. This window is the Windows and Linux shell — that pipeline is not connected here yet.".to_string(),
        },
        health: EmptyPane {
            body: "Coverage, freshness, and yield are measured from capture. Nothing is measured on this OS yet, so this pane stays empty rather than showing zeroes.".to_string(),
        },
        sources: EmptyPane {
            body: "Mail, calendar, and the other connectors authenticate on the device and sync through the core. They are not offered here until that core runs on this OS.".to_string(),
        },
        memory: EmptyPane {
            body: "People, commitments, and open loops live in the on-device memory store. This shell does not open that store yet.".to_string(),
        },
        activity: EmptyPane {
            body: "L1/L2/L3 runs will list here once the agent engine is wired on this OS. Send and post stay on explicit confirm.".to_string(),
        },
        trace: EmptyPane {
            body: "Every off-device chunk gets a row. This process does not send, so the ledger is empty.".to_string(),
        },
    }
}

fn current_os() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "other"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_copy_does_not_invent_metrics() {
        let v = assemble();
        for body in [
            &v.today.body,
            &v.health.body,
            &v.sources.body,
            &v.memory.body,
            &v.activity.body,
            &v.trace.body,
        ] {
            assert!(!body.contains('%'), "{body}");
            assert!(!body.to_lowercase().contains("ai-powered"), "{body}");
            assert!(!body.to_lowercase().contains("second brain"), "{body}");
        }
        assert!(v.health.body.contains("zeroes"));
        assert!(v.trace.body.contains("does not send"));
    }

    #[test]
    fn os_label_is_this_target() {
        let v = assemble();
        if cfg!(windows) {
            assert_eq!(v.os, "windows");
        }
        if cfg!(target_os = "linux") {
            assert_eq!(v.os, "linux");
        }
    }
}
