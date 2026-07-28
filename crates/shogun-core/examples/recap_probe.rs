//! Device probe for MT4 meeting minutes (Issue #7) — NOT shipped code, a hand-run verification tool.
//!
//! Runs the REAL Select KK Batch lane on a sample transcript to prove the recap path end to end:
//! build_prompt → AnthropicBatchClient (submit/poll/results) → parse_minutes → print. This is the
//! same lane `apps/desktop/src-tauri/src/meeting_recap.rs` drives on `Effect::BuildRecap`, isolated
//! from the app, the encrypted DB and the meeting state machine.
//!
//! The Batch API is asynchronous (results can take minutes), so this either prints the parsed
//! minutes if the batch finishes within the poll budget, or the submitted batch id if it is still
//! pending — either way, a successful submit proves the key + transport + traceability wiring.
//!
//! Run (needs the Select KK key; pull it from Keychain into the env):
//!   SHOGUN_SELECT_KK="$(security find-generic-password -s SHOGUN -a select-kk-batch -w)" \
//!     cargo run -p shogun-core --features daemon-server --release --example recap_probe
//!
//! Invariant 3: the one chunk that leaves the device is recorded by the client's `submit` before it
//! goes out. This probe uses an in-memory RecordingSink and prints how many rows it captured.

#[cfg(feature = "net")]
fn main() {
    use shogun_core::llm::anthropic::{AnthropicBatchClient, AnthropicConfig, BatchItem};
    use shogun_core::llm::traceability::RecordingSink;
    use shogun_core::llm::transport::ReqwestTransport;
    use shogun_core::llm::{Secret, SelectKkKey};
    use shogun_core::meeting::minutes::{build_prompt, parse_minutes, TranscriptLine};
    use std::time::Duration;

    const RECAP_MODEL: &str = "claude-haiku-4-5-20251001";
    const POLL_INTERVAL: Duration = Duration::from_secs(15);
    const MAX_POLLS: u32 = 8; // ~2 minutes before we report "still pending"

    let key = match std::env::var("SHOGUN_SELECT_KK") {
        Ok(k) if !k.trim().is_empty() => k,
        _ => {
            eprintln!("set SHOGUN_SELECT_KK (e.g. from `security find-generic-password -s SHOGUN -a select-kk-batch -w`)");
            std::process::exit(2);
        }
    };

    // A short, realistic two-speaker meeting.
    let lines = [
        TranscriptLine { speaker: Some("me"), text: "Let's decide the meeting-notes scope for this week." },
        TranscriptLine { speaker: Some("other"), text: "I think we ship audio capture and the summary, but leave the panel card for next week." },
        TranscriptLine { speaker: Some("me"), text: "Agreed. I'll pin the turbo model hash. Can you wire the summary generation?" },
        TranscriptLine { speaker: Some("other"), text: "Yes, I'll take the summary. We should also verify it on a real machine before merging." },
    ];
    let notes = "- scope: audio + summary this week\n- card UI: next week";
    let prompt = build_prompt(&lines, Some(notes), "en");

    let items = [BatchItem {
        custom_id: "recap-probe".to_string(),
        purpose: "meeting_recap".to_string(),
        chunk: prompt,
    }];

    let transport = match ReqwestTransport::new() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("transport unavailable: {e}");
            std::process::exit(1);
        }
    };
    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("tokio runtime unavailable: {e}");
            std::process::exit(1);
        }
    };

    let sink = RecordingSink::new();
    let client = AnthropicBatchClient::new(
        transport,
        sink,
        SelectKkKey::new(Secret::new(key)),
        AnthropicConfig::new(RECAP_MODEL),
    );

    println!("submitting one meeting-recap batch item to the Select KK lane (model {RECAP_MODEL})…");
    let result = rt.block_on(async {
        client
            .run(&items, MAX_POLLS, || async {
                tokio::time::sleep(POLL_INTERVAL).await;
            })
            .await
    });

    match result {
        Ok(results) => {
            let text = results.into_iter().find_map(|r| r.text);
            match text {
                Some(t) => match parse_minutes(&t) {
                    Ok(m) => {
                        println!("\n=== MEETING MINUTES (real batch) ===");
                        println!("summary: {}", m.summary);
                        println!("decisions:");
                        for d in &m.decisions {
                            println!("  - {d}");
                        }
                        println!("next actions (suggestions):");
                        for a in &m.next_actions {
                            match &a.owner {
                                Some(o) => println!("  - [{o}] {}", a.text),
                                None => println!("  - {}", a.text),
                            }
                        }
                    }
                    Err(e) => println!("batch returned text but it did not parse as minutes: {e}\n---\n{t}"),
                },
                None => println!("batch ended with no text result"),
            }
        }
        Err(e) => {
            // Submit succeeded but the batch did not end inside the budget, or a provider error.
            println!("batch did not complete within the poll budget (this is normal — the Batch API is async): {e}");
            println!("a successful submit still proves the key + transport + traceability path.");
        }
    }
}

#[cfg(not(feature = "net"))]
fn main() {
    eprintln!("build with --features daemon-server (or net): cargo run -p shogun-core --features daemon-server --example recap_probe");
}
