//! JSONL record schema (spec §4.4).
//!
//! Every line is `{"ts","mono","v","type","payload"}`. Payload types are designed so
//! that **no captured text or window-title body can be represented** — where text must
//! be referenced, only `text_bytes` + `text_xxh64` exist (see [`crate::digest`]). This is
//! the type-level enforcement of the CLAUDE.md privacy rule; `tests::no_text_body_fields`
//! guards it.

use serde::Serialize;

/// Which UI mode produced the sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Notch,
    Pseudo,
}

/// Cache-update trigger class (spec §3.10.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheTrigger {
    AppSwitch,
    WindowSwitch,
    TitleChange,
}

/// Why an Expanded session closed (spec §4.2.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseReason {
    Timeout,
    Esc,
    OutsideClick,
    Forced,
}

/// Q2 — expand latency (spec §4.2.1).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ExpandLatency {
    pub latency_ms: f64,
    pub total_perceived_ms: f64,
    pub hover_enter_offset_ms: f64,
    pub mode: Mode,
    pub fullscreen: bool,
    pub display_count: u32,
}

/// Q3-A — context cache update (spec §4.2.2). Carries digest only, never text.
#[derive(Debug, Clone, Serialize)]
pub struct CacheUpdate {
    pub latency_ms: f64,
    pub trigger: CacheTrigger,
    pub bundle_id: String,
    pub text_bytes: usize,
    pub text_xxh64: String,
    pub elements_visited: u32,
    pub depth_reached: u32,
    pub partial: bool,
    pub truncated: bool,
    pub cancelled: bool,
}

impl CacheUpdate {
    /// Build from the raw captured `text`, storing only its digest. The text is
    /// borrowed and dropped here — it is the single choke point that keeps bodies
    /// out of records.
    #[allow(clippy::too_many_arguments)]
    pub fn from_text(
        latency_ms: f64,
        trigger: CacheTrigger,
        bundle_id: impl Into<String>,
        text: &str,
        elements_visited: u32,
        depth_reached: u32,
        partial: bool,
        truncated: bool,
        cancelled: bool,
    ) -> Self {
        let (text_bytes, text_xxh64) = crate::digest::text_digest(text);
        Self {
            latency_ms,
            trigger,
            bundle_id: bundle_id.into(),
            text_bytes,
            text_xxh64,
            elements_visited,
            depth_reached,
            partial,
            truncated,
            cancelled,
        }
    }
}

/// Q3-B — one CPU sample (spec §4.2.3).
#[derive(Debug, Clone, Serialize)]
pub struct CpuSample {
    /// Instantaneous percent (1 core = 100%).
    pub cpu_pct: f64,
    /// 1-minute moving average once the window is full.
    pub cpu_1min_avg: Option<f64>,
    /// Sampling method, recorded on every sample so a run uses exactly one (spec §4.2.3).
    pub method: &'static str,
    pub rss_mb: f64,
}

/// Interaction tallies inside one Expanded session.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Interactions {
    pub clicks: u32,
    pub keys: u32,
    pub scrolls: u32,
}

/// Q4 — one Expanded session (spec §4.2.4).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ExpandSession {
    pub opened_at_ms: u64,
    pub closed_at_ms: u64,
    pub duration_ms: u64,
    pub interactions: Interactions,
    pub close_reason: CloseReason,
    pub auto_false_positive: bool,
    pub manual_false_positive: bool,
}

/// Q1 — 60s soak heartbeat (spec §4.5).
#[derive(Debug, Clone, Serialize)]
pub struct Heartbeat {
    pub panel_visible: bool,
    pub panel_frame_ok: bool,
    pub state: String,
    pub cpu_1min_avg: f64,
    pub rss_mb: f64,
    pub ax_calls_total: u64,
    pub uptime_s: u64,
}

/// State-machine transition (spec §3.3).
#[derive(Debug, Clone, Serialize)]
pub struct StateTransition {
    pub from: String,
    pub to: String,
    pub trigger: String,
}

/// Notch/pseudo geometry captured at startup and after display changes (spec §3.2).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct NotchGeometry {
    pub mode: Mode,
    pub notch_w: f64,
    pub notch_h: f64,
    pub left_aux_w: f64,
    pub right_aux_w: f64,
    pub menubar_h: f64,
}

/// The record body — the `type`/`payload` pair. Adding a variant with a text body
/// field would break `no_text_body_fields`; keep bodies out.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "payload")]
pub enum Body {
    #[serde(rename = "metric.expand_latency")]
    ExpandLatency(ExpandLatency),
    #[serde(rename = "metric.cache_update")]
    CacheUpdate(CacheUpdate),
    #[serde(rename = "metric.cpu_sample")]
    CpuSample(CpuSample),
    #[serde(rename = "event.expand_session")]
    ExpandSession(ExpandSession),
    #[serde(rename = "event.state_transition")]
    StateTransition(StateTransition),
    #[serde(rename = "event.notch_geometry")]
    NotchGeometry(NotchGeometry),
    #[serde(rename = "soak.heartbeat")]
    Heartbeat(Heartbeat),
    #[serde(rename = "counter.top_band_entry")]
    TopBandEntry { count: u64 },
    #[serde(rename = "event.anim_timeout")]
    AnimTimeout { state: String },
    #[serde(rename = "event.panel_recovered")]
    PanelRecovered { method: String, recover_ms: u64 },
}

/// One JSONL line. `ts` = epoch ms, `mono` = ns since process start.
#[derive(Debug, Clone, Serialize)]
pub struct Record {
    pub ts: u64,
    pub mono: u64,
    pub v: u8,
    #[serde(flatten)]
    pub body: Body,
}

impl Record {
    pub fn new(ts_epoch_ms: u64, mono_ns: u64, body: Body) -> Self {
        Self { ts: ts_epoch_ms, mono: mono_ns, v: 1, body }
    }

    /// Serialize to a single JSONL line (no trailing newline).
    pub fn to_line(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_records() -> Vec<Record> {
        vec![
            Record::new(
                1,
                10,
                Body::ExpandLatency(ExpandLatency {
                    latency_ms: 62.4,
                    total_perceived_ms: 168.1,
                    hover_enter_offset_ms: 105.0,
                    mode: Mode::Notch,
                    fullscreen: false,
                    display_count: 2,
                }),
            ),
            Record::new(
                2,
                20,
                Body::CacheUpdate(CacheUpdate::from_text(
                    141.0,
                    CacheTrigger::AppSwitch,
                    "com.apple.Safari",
                    "SECRET USER TEXT that must never be logged",
                    211,
                    6,
                    false,
                    false,
                    false,
                )),
            ),
            Record::new(
                3,
                30,
                Body::Heartbeat(Heartbeat {
                    panel_visible: true,
                    panel_frame_ok: true,
                    state: "Idle".into(),
                    cpu_1min_avg: 2.1,
                    rss_mb: 184.0,
                    ax_calls_total: 1420,
                    uptime_s: 8183,
                }),
            ),
        ]
    }

    #[test]
    fn envelope_shape_matches_spec() {
        let r = &sample_records()[0];
        let val: serde_json::Value = serde_json::from_str(&r.to_line().unwrap()).unwrap();
        assert_eq!(val["type"], "metric.expand_latency");
        assert_eq!(val["v"], 1);
        assert!(val["ts"].is_number());
        assert!(val["mono"].is_number());
        assert!(val["payload"].is_object());
        assert_eq!(val["payload"]["latency_ms"], 62.4);
    }

    #[test]
    fn cache_update_stores_digest_not_text() {
        let r = &sample_records()[1];
        let line = r.to_line().unwrap();
        assert!(line.contains("text_bytes"));
        assert!(line.contains("text_xxh64"));
        // The raw text must not survive into the record.
        assert!(!line.contains("SECRET USER TEXT"));
    }

    /// Privacy guardrail: no record variant may serialize a raw text/title body.
    #[test]
    fn no_text_body_fields() {
        const FORBIDDEN: &[&str] = &["\"text\":", "\"title\":", "\"window_title\":", "\"body\":"];
        for r in sample_records() {
            let line = r.to_line().unwrap();
            for f in FORBIDDEN {
                assert!(!line.contains(f), "record leaked forbidden field {f}: {line}");
            }
        }
    }
}
