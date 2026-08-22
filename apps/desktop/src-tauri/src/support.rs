//! CS / bug-report 窓口 — the Help & Support panel's submit path.
//!
//! The webview collects category + message (+ optional reply email, + an explicit diagnostics
//! opt-in) and this module does the rest: it assembles the diagnostics tuple on the Rust side
//! (the webview cannot inject arbitrary fields — invariant 1: data decisions live in the core),
//! posts through `shogun_core::support_client` (FR-TR-03: the one raw HTTP client lives in
//! shogun-core), and records the send on the egress ledger (`Route::Support`, invariant 3).
//!
//! What can go out: the user's own words, their optional email, and — only when they ticked the
//! box — app version / macOS version / plan name. Never capture content, never memory content,
//! never a licence key.

#![allow(dead_code)]

#[cfg(target_os = "macos")]
pub mod mac {
    use shogun_core::support_client::{self, SupportReport};
    use tauri::{AppHandle, Manager};

    /// Mirror of the server's closed category set. Enforced here too so a compromised webview
    /// cannot use the category field as a free-text channel.
    const CATEGORIES: [&str; 3] = ["bug", "feedback", "question"];
    /// Server-side bounds (apps/website/src/lib/support.ts), enforced before the network.
    const MIN_MESSAGE_CHARS: usize = 5;
    const MAX_MESSAGE_CHARS: usize = 4000;
    const MAX_EMAIL_CHARS: usize = 254;

    /// The running macOS version ("14.5"), via NSProcessInfo — no process spawn.
    fn os_version() -> String {
        use objc2_foundation::NSProcessInfo;
        let v = NSProcessInfo::processInfo().operatingSystemVersion();
        if v.patchVersion == 0 {
            format!("{}.{}", v.majorVersion, v.minorVersion)
        } else {
            format!("{}.{}.{}", v.majorVersion, v.minorVersion, v.patchVersion)
        }
    }

    /// The plan name for the diagnostics tuple — the same resolution every gate uses
    /// (entitlement.rs), reported as a string, entitling nothing.
    fn plan_label(app: &AppHandle) -> &'static str {
        use shogun_agents::entitlement::PlanStatus;
        match crate::entitlement::mac::current(app).status {
            PlanStatus::Trial => "trial",
            PlanStatus::TrialExpired => "trial_expired",
            PlanStatus::Standard => "standard",
            PlanStatus::Pro => "pro",
        }
    }

    /// Record the send on the egress ledger (invariant 3). Called only after a successful
    /// submit — like the L3 send path, a report that never left produces no row. The digest is
    /// over the JSON body that went out; the ledger keeps bytes + digest, never the text.
    fn record_support_egress(app: &AppHandle, sent_body: &str) {
        use shogun_core::llm::traceability::{Route, TraceRecord, TraceabilitySink};
        let Some(db) = app.try_state::<shogun_core::daemon::Db>() else {
            return;
        };
        let origin = crate::billing::mac::api_origin();
        let host = origin
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or("")
            .to_string();
        db.traceability_sink().record(TraceRecord::for_chunk(
            Route::Support,
            "support_report",
            host,
            sent_body,
            false,
        ));
    }

    /// Validate and send one report. Returns the server's ticket id.
    ///
    /// Async so the webview stays responsive; the blocking HTTP call runs on a blocking thread,
    /// never on a tokio worker or the main thread.
    #[tauri::command]
    pub async fn support_submit_report(
        app: AppHandle,
        category: String,
        message: String,
        email: Option<String>,
        include_diagnostics: bool,
    ) -> Result<String, String> {
        if !CATEGORIES.contains(&category.as_str()) {
            return Err("unknown category".to_string());
        }
        let message = message.trim().to_string();
        if message.chars().count() < MIN_MESSAGE_CHARS {
            return Err("message is too short".to_string());
        }
        if message.chars().count() > MAX_MESSAGE_CHARS {
            return Err(format!("message is over {MAX_MESSAGE_CHARS} characters"));
        }
        let email = email
            .map(|e| e.trim().to_string())
            .filter(|e| !e.is_empty());
        if let Some(e) = &email {
            // Shape check only — the server does the real validation.
            if !e.contains('@') || e.len() > MAX_EMAIL_CHARS {
                return Err("that email address doesn't look right".to_string());
            }
        }

        // Diagnostics assembled core-side, only under the explicit opt-in.
        let (app_version, os_ver, plan) = if include_diagnostics {
            (
                Some(app.package_info().version.to_string()),
                Some(os_version()),
                Some(plan_label(&app).to_string()),
            )
        } else {
            (None, None, None)
        };

        let report = SupportReport {
            category,
            message,
            email,
            app_version,
            os_version: os_ver,
            plan,
        };
        let sent_body = report.to_json().to_string();
        let origin = crate::billing::mac::api_origin();

        let ticket_id = tauri::async_runtime::spawn_blocking(move || {
            support_client::submit(&origin, &report)
        })
        .await
        .map_err(|e| format!("support submit task failed: {e}"))?
        .map_err(|e| e.message())?;

        record_support_egress(&app, &sent_body);
        Ok(ticket_id)
    }
}
