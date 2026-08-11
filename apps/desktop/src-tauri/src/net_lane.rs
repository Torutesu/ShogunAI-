//! The shared HTTPS lane: one connection pool and one tokio runtime for every provider call the
//! user is actually waiting on.
//!
//! Before this, each latency path built both per call — a fresh `reqwest::Client` and a fresh
//! current-thread runtime for every chat turn, every ⌥-tap draft and every live-translated
//! subtitle. A fresh client starts with an empty pool, so each of those re-paid DNS + TCP + TLS
//! to the provider before the model saw a single byte. On a normal link that handshake is most of
//! the budget the "first token in 1s" SLO (CLAUDE.md §SLO) has to fit inside.
//!
//! **The two must be taken together.** A pooled connection is driven by a background task on the
//! runtime that opened it: hand the shared client to a short-lived runtime and every connection
//! it pools dies when that runtime drops, leaving stale entries for the next caller to trip over.
//! `lane` returns the pair so a call site cannot get that wrong by accident, and the paths that
//! stay on private clients (`dream`, `meeting_recap` — nightly and once-a-meeting, where a
//! handshake costs nothing anyone can feel) keep their own runtimes to match.
//!
//! Nothing here changes what is sent. It changes how often the connection under it is rebuilt.
#![allow(dead_code)]

#[cfg(target_os = "macos")]
pub use mac::lane;

#[cfg(target_os = "macos")]
pub mod mac {
    use shogun_core::llm::transport::ReqwestTransport;

    /// The runtime the shared pool's connections live on. Built once, kept for the process.
    static RUNTIME: std::sync::OnceLock<Option<tokio::runtime::Runtime>> =
        std::sync::OnceLock::new();

    /// Two worker threads: enough that one lane's request is never queued behind another's, few
    /// enough to stay inside the 5% idle-CPU budget (idle workers park).
    pub fn runtime() -> Option<&'static tokio::runtime::Runtime> {
        RUNTIME
            .get_or_init(|| {
                let built = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .thread_name("shogun-net")
                    .enable_all()
                    .build();
                match built {
                    Ok(rt) => Some(rt),
                    Err(e) => {
                        eprintln!("[net] shared runtime unavailable: {e}");
                        None
                    }
                }
            })
            .as_ref()
    }

    /// A handle on the process-wide connection pool.
    pub fn transport() -> Option<ReqwestTransport> {
        match ReqwestTransport::shared() {
            Ok(t) => Some(t),
            Err(e) => {
                eprintln!("[net] shared transport unavailable: {e}");
                None
            }
        }
    }

    /// The pair, or `None` if either could not be built. Callers that need both should take them
    /// from here rather than assembling them separately — see the module docs for why.
    pub fn lane() -> Option<(ReqwestTransport, &'static tokio::runtime::Runtime)> {
        Some((transport()?, runtime()?))
    }
}
