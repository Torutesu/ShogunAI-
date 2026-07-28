# MT3 音声レーン（オンデバイス ASR）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 会議 `Recording` 中にマイク＋システム音声をオンデバイスで文字起こしし、`transcript_segments` に蓄積する（音声はRAMのみ・ファイル化しない）。

**Architecture:** shogun-core に `audio` モジュールを追加。純ロジック層（ring / vad / resample / Transcriber trait / worker）は依存軽量で Linux CI テスト可能にし、FFI層（cpal マイク / Core Audio tap / whisper-rs）は `audio` feature ＋ `#[cfg(target_os="macos")]` に分離。既に statemachine が発行している `StartAudio`/`StopAudio` Effect を `apps/desktop/src-tauri/src/meeting.rs` の `apply()` で消費して worker を起動/停止する。

**Tech Stack:** Rust / rusqlite (refinery migration) / whisper-rs (whisper.cpp, Metal) / cpal / objc2 + core-audio-sys / webrtc-vad or 自前エネルギーVAD / rubato。

**Spec:** `docs/superpowers/specs/2026-07-28-meeting-audio-asr-design.md`

---

## ファイル構成

作成/変更するファイルと責務：

| ファイル | 種別 | 責務 |
|---|---|---|
| `crates/shogun-memory/src/migrations/V9__transcript_segments.sql` | 作成 | テーブル新設 |
| `crates/shogun-memory/src/transcript_segments.rs` | 作成 | 挿入/取得 API |
| `crates/shogun-memory/src/lib.rs` | 変更 | `pub mod transcript_segments;` 追加 |
| `crates/shogun-core/src/audio/mod.rs` | 作成 | サブモジュール宣言＋`Speaker`/`Utterance`/`Segment`型 |
| `crates/shogun-core/src/audio/ring.rs` | 作成 | 音源別RAMリングバッファ（30s上限・drop-oldest） |
| `crates/shogun-core/src/audio/resample.rs` | 作成 | 任意SR→16kHz mono f32 |
| `crates/shogun-core/src/audio/vad.rs` | 作成 | 発話区間の切り出し |
| `crates/shogun-core/src/audio/asr/mod.rs` | 作成 | `trait Transcriber`＋`FakeTranscriber` |
| `crates/shogun-core/src/audio/asr/whisper.rs` | 作成 | whisper-rs 実装（feature `audio`, macOS） |
| `crates/shogun-core/src/audio/capture/mod.rs` | 作成 | `trait AudioSource`＋`FakeSource` |
| `crates/shogun-core/src/audio/capture/mic.rs` | 作成 | cpal マイク（feature `audio`, macOS） |
| `crates/shogun-core/src/audio/capture/system_tap.rs` | 作成 | Core Audio process tap（feature `audio`, macOS 14.4+） |
| `crates/shogun-core/src/audio/worker.rs` | 作成 | capture+VAD+ASRを束ねDB書込 |
| `crates/shogun-core/src/lib.rs` | 変更 | `pub mod audio;` 追加 |
| `crates/shogun-core/Cargo.toml` | 変更 | `audio` feature＋依存追加 |
| `crates/shogun-core/src/meeting/settings.rs` | 変更 | `asr_model` 設定追加 |
| `apps/desktop/src-tauri/src/meeting.rs` | 変更 | `StartAudio`/`StopAudio` で worker 起動/停止 |
| `apps/desktop/src-tauri/Cargo.toml` | 変更 | shogun-core の `audio` feature を有効化 |
| `docs/migrations/V9-rollback.md` | 作成 | ロールバック手順 |
| `docs/meeting-notes-ui-design.md` | 変更 | §7.1 進捗表の MT3 更新 |

**設計原則：** worker は Transcriber/AudioSource の trait に対して書く。純ロジックは fake 実装で決定論的にテストし、実 FFI（cpal/objc2/whisper）は feature gate 内に隔離する。

---

## Task 1: V9 マイグレーション（transcript_segments テーブル）

**Files:**
- Create: `crates/shogun-memory/src/migrations/V9__transcript_segments.sql`
- Test: `crates/shogun-memory/src/transcript_segments.rs`（Task 2でテスト追加）

- [ ] **Step 1: マイグレーションSQLを書く**

`crates/shogun-memory/src/migrations/V9__transcript_segments.sql`:

```sql
-- The transcript of a meeting, as text only (FR-MT-13, §6.16.2). Additive: one new table.
--
-- Invariant 2: audio is processed on-device and never stored — the waveform lives only in a RAM
-- ring buffer and is discarded after ASR. What persists is this text plus its provenance, nothing
-- that can reconstruct the sound.
--
-- `speaker` is 'me' (microphone) or 'other' (system tap); NULL means unknown and is never guessed.
-- `origin` is 'asr' here; 'caption' is reserved for a future path that reads the meeting UI's own
-- captions, which carries a different consent story (§5), so the column exists from the start.
-- `confidence` is the model's own certainty, normalised to [0,1] — a low-confidence line must not
-- be presented downstream as fact (data-model principle).

CREATE TABLE transcript_segments (
    id         INTEGER PRIMARY KEY,
    session_id INTEGER NOT NULL REFERENCES sessions (id),
    ts         INTEGER NOT NULL,
    speaker    TEXT,
    text       TEXT    NOT NULL,
    origin     TEXT    NOT NULL,
    confidence REAL    NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    created_at INTEGER NOT NULL
) STRICT;

CREATE INDEX idx_transcript_session ON transcript_segments (session_id, ts);
```

- [ ] **Step 2: マイグレーションが適用されることを確認**

Run: `cargo test -p shogun-memory migrations_apply 2>&1 | tail -20`
（既存のマイグレーション適用テストがV9まで通ればOK。無ければ `cargo test -p shogun-memory` 全体が緑であることを確認。）
Expected: PASS（`refinery_schema_history` に V9 が記録される）

- [ ] **Step 3: コミット**

```bash
git add crates/shogun-memory/src/migrations/V9__transcript_segments.sql
git commit -m "feat(memory): transcript_segments テーブル (V9) (#7)"
```

---

## Task 2: transcript_segments リポジトリ API

**Files:**
- Create: `crates/shogun-memory/src/transcript_segments.rs`
- Modify: `crates/shogun-memory/src/lib.rs`（`pub mod transcript_segments;` を `pub mod thread;` の直前アルファベット順位置に追加）

- [ ] **Step 1: 失敗するテストを書く**

`crates/shogun-memory/src/transcript_segments.rs`（ファイル全体を作成）:

```rust
//! The meeting transcript, stored as text only (FR-MT-13).
//!
//! Invariant 2: the audio itself is never persisted. This module is the *only* writer of the
//! transcript, and it writes text that has already been through `redact` — a spoken credential
//! ("my password is …") is as sensitive on the write path as a typed one, so the same masking
//! that protects captured screen text protects the transcript.

use rusqlite::{params, Connection};

/// Who spoke, decided by the capture source rather than by inference: microphone input is `Me`,
/// the system tap is `Other`. `Unknown` is stored as NULL — we never guess a speaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speaker {
    Me,
    Other,
    Unknown,
}

impl Speaker {
    fn as_str(self) -> Option<&'static str> {
        match self {
            Speaker::Me => Some("me"),
            Speaker::Other => Some("other"),
            Speaker::Unknown => None,
        }
    }
}

/// One transcribed line, ready to persist.
#[derive(Debug, Clone, PartialEq)]
pub struct NewSegment<'a> {
    pub session_id: i64,
    pub ts: i64,
    pub speaker: Speaker,
    pub text: &'a str,
    pub confidence: f64,
}

/// Append one transcript line. `origin` is fixed to `'asr'` here; the caption path is a future
/// caller. Returns the new row id.
pub fn append(conn: &Connection, seg: &NewSegment, now: i64) -> Result<i64, rusqlite::Error> {
    let redacted = crate::redact::redact(seg.text);
    conn.execute(
        "INSERT INTO transcript_segments
           (session_id, ts, speaker, text, origin, confidence, created_at)
         VALUES (?1, ?2, ?3, ?4, 'asr', ?5, ?6)",
        params![
            seg.session_id,
            seg.ts,
            seg.speaker.as_str(),
            redacted.as_ref(),
            seg.confidence,
            now,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// All lines for a session in time order. Recap (MT4) reads through this.
pub fn for_session(
    conn: &Connection,
    session_id: i64,
) -> Result<Vec<(i64, Option<String>, String, f64)>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT ts, speaker, text, confidence FROM transcript_segments
         WHERE session_id = ?1 ORDER BY ts, id",
    )?;
    let rows = stmt.query_map([session_id], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{open, NewSession};

    fn session(conn: &Connection) -> i64 {
        open(
            conn,
            &NewSession {
                kind: "meeting",
                started_at: 1_000,
                title: Some("Weekly sync"),
                app_bundle_id: Some("us.zoom.xos"),
                calendar_occurrence_id: None,
                confidence: 0.65,
                provenance: "{}",
            },
        )
        .unwrap()
    }

    #[test]
    fn segments_are_read_back_in_time_order() {
        let conn = crate::open_in_memory().unwrap();
        let sid = session(&conn);
        append(&conn, &NewSegment { session_id: sid, ts: 2_000, speaker: Speaker::Other, text: "second", confidence: 0.9 }, 9).unwrap();
        append(&conn, &NewSegment { session_id: sid, ts: 1_000, speaker: Speaker::Me, text: "first", confidence: 0.8 }, 9).unwrap();
        let got = for_session(&conn, sid).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].2, "first");
        assert_eq!(got[1].2, "second");
    }

    #[test]
    fn unknown_speaker_is_stored_as_null() {
        let conn = crate::open_in_memory().unwrap();
        let sid = session(&conn);
        append(&conn, &NewSegment { session_id: sid, ts: 1_000, speaker: Speaker::Unknown, text: "hi", confidence: 0.5 }, 9).unwrap();
        let got = for_session(&conn, sid).unwrap();
        assert_eq!(got[0].1, None);
    }

    #[test]
    fn spoken_secrets_are_redacted_before_write() {
        // A transcript is captured text, not the user's own note — the same masking that protects
        // screen capture protects it. "sk-ant-" is a known issuer prefix in redact.
        let conn = crate::open_in_memory().unwrap();
        let sid = session(&conn);
        append(&conn, &NewSegment { session_id: sid, ts: 1_000, speaker: Speaker::Me, text: "the key is sk-ant-abc123def456", confidence: 0.9 }, 9).unwrap();
        let got = for_session(&conn, sid).unwrap();
        assert!(!got[0].2.contains("sk-ant-abc123def456"), "raw secret leaked into transcript");
    }
}
```

`crates/shogun-memory/src/lib.rs` の module 宣言に追加（`pub mod thread;` の直前）:

```rust
pub mod transcript_segments;
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test -p shogun-memory transcript_segments 2>&1 | tail -20`
Expected: 最初は redact のシンボル解決やコンパイルで FAIL しうる。`crate::redact::redact` が存在することは確認済み（`redact.rs:71`）。コンパイルが通れば緑になるはず。

- [ ] **Step 3: テストが通ることを確認**

Run: `cargo test -p shogun-memory transcript_segments 2>&1 | tail -20`
Expected: PASS（3テスト）

- [ ] **Step 4: clippy**

Run: `cargo clippy -p shogun-memory --all-targets 2>&1 | tail -10`
Expected: warnings なし

- [ ] **Step 5: コミット**

```bash
git add crates/shogun-memory/src/transcript_segments.rs crates/shogun-memory/src/lib.rs
git commit -m "feat(memory): transcript_segments の挿入/取得API (#7)"
```

---

## Task 3: audio モジュール骨組みと共有型

**Files:**
- Create: `crates/shogun-core/src/audio/mod.rs`
- Modify: `crates/shogun-core/src/lib.rs`（`pub mod audio;` を `pub mod bus;` の直前に追加）

- [ ] **Step 1: 共有型とモジュール宣言を書く**

`crates/shogun-core/src/audio/mod.rs`:

```rust
//! The meeting audio lane (MT3, FR-MT-13): microphone + system audio → on-device ASR → text.
//!
//! Invariant 2 is the design's spine: the waveform lives only in a RAM ring buffer and is
//! discarded after transcription. Nothing here writes samples to a file, and the ASR engine is
//! fed an in-memory `&[f32]` slice — a path that requires a temp file is not chosen.
//!
//! The pure-logic pieces (`ring`, `resample`, `vad`, the `Transcriber`/`AudioSource` traits, and
//! `worker`) are dependency-light and unit-tested on Linux CI with fakes. The real FFI backends
//! (`capture::mic`, `capture::system_tap`, `asr::whisper`) live behind the `audio` feature and
//! `#[cfg(target_os = "macos")]`, mirroring how `db`/`net` isolate their heavy deps.

pub mod asr;
pub mod capture;
pub mod resample;
pub mod ring;
pub mod vad;
pub mod worker;

/// Who is speaking, decided by capture source: mic = `Me`, system tap = `Other`. Re-exported as
/// the same idea `shogun-memory` persists, but kept as its own type so core does not depend on the
/// memory crate for pure-logic tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speaker {
    Me,
    Other,
}

/// 16 kHz mono is the rate every backend and the VAD agree on, fixed here so the number lives in
/// one place.
pub const SAMPLE_RATE: u32 = 16_000;

/// A span of speech cut out by the VAD, ready for ASR. `pcm` is 16 kHz mono f32 and owned so the
/// capture thread can reuse its own buffers immediately.
#[derive(Debug, Clone, PartialEq)]
pub struct Utterance {
    pub speaker: Speaker,
    /// epoch ms at the first sample.
    pub started_at: i64,
    pub pcm: Vec<f32>,
}

/// One line back from a `Transcriber`.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub text: String,
    /// Model certainty, already normalised to [0,1].
    pub confidence: f64,
}
```

`crates/shogun-core/src/lib.rs` に追加（`pub mod bus;` の直前）:

```rust
pub mod audio;
```

- [ ] **Step 2: コンパイル確認（空モジュールなので一旦 stub）**

この時点では下位モジュールが未作成でコンパイルが通らない。Task 4–8 の各モジュールを空 `//! stub` で先に置くのではなく、**Task 4 以降で1つずつ実装しコンパイルを緑にする**。まず `ring`/`resample`/`vad`/`asr`/`capture`/`worker` の各ファイルを最小の空実装で作る：

各ファイルに一時的に以下を置く（Task 4–8 で中身を実装）:
```rust
//! stub — implemented in a later task.
```
そして `cargo build -p shogun-core 2>&1 | tail -20` が通ることを確認。
Expected: PASS

- [ ] **Step 3: コミット**

```bash
git add crates/shogun-core/src/audio/ crates/shogun-core/src/lib.rs
git commit -m "feat(core): audio モジュール骨組みと共有型 (#7)"
```

---

## Task 4: リングバッファ（RAM・30s上限・drop-oldest）

**Files:**
- Create/Replace: `crates/shogun-core/src/audio/ring.rs`

- [ ] **Step 1: 失敗するテストを書く**

`crates/shogun-core/src/audio/ring.rs`（stubを置換）:

```rust
//! A fixed-capacity RAM buffer of PCM samples that drops the oldest when full (§2).
//!
//! The 30s cap is a design wall, not a tuning knob: without it, "keep what ASR hasn't caught up
//! on" becomes "keep everything", which is a recording. When the writer outruns the reader the
//! oldest audio is discarded, never spilled to disk.

use super::SAMPLE_RATE;

/// Seconds of audio kept at most. 30s matches §2.
pub const MAX_SECONDS: usize = 30;

pub struct Ring {
    buf: std::collections::VecDeque<f32>,
    cap: usize,
}

impl Ring {
    /// A ring holding at most `MAX_SECONDS` of 16 kHz mono audio.
    pub fn new() -> Self {
        let cap = MAX_SECONDS * SAMPLE_RATE as usize;
        Ring { buf: std::collections::VecDeque::with_capacity(cap), cap }
    }

    /// Append samples, dropping the oldest to stay within capacity.
    pub fn push(&mut self, samples: &[f32]) {
        self.buf.extend(samples.iter().copied());
        while self.buf.len() > self.cap {
            self.buf.pop_front();
        }
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Drain everything currently buffered as a contiguous Vec, leaving the ring empty. Used at
    /// stop to flush the final utterance before the PCM is discarded.
    pub fn drain(&mut self) -> Vec<f32> {
        self.buf.drain(..).collect()
    }
}

impl Default for Ring {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_exceeds_capacity() {
        let mut r = Ring::new();
        let cap = MAX_SECONDS * SAMPLE_RATE as usize;
        r.push(&vec![0.1_f32; cap + 5_000]);
        assert_eq!(r.len(), cap, "ring grew past the 30s wall");
    }

    #[test]
    fn drops_oldest_first() {
        let mut r = Ring { buf: std::collections::VecDeque::new(), cap: 3 };
        r.push(&[1.0, 2.0, 3.0]);
        r.push(&[4.0]); // 1.0 should fall off the front
        assert_eq!(r.drain(), vec![2.0, 3.0, 4.0]);
    }

    #[test]
    fn drain_empties() {
        let mut r = Ring::new();
        r.push(&[1.0, 2.0]);
        assert_eq!(r.drain(), vec![1.0, 2.0]);
        assert!(r.is_empty());
    }
}
```

- [ ] **Step 2: テストが失敗→通ることを確認**

Run: `cargo test -p shogun-core audio::ring 2>&1 | tail -15`
Expected: PASS（3テスト）

- [ ] **Step 3: clippy＋コミット**

```bash
cargo clippy -p shogun-core --all-targets 2>&1 | tail -5
git add crates/shogun-core/src/audio/ring.rs
git commit -m "feat(core): 音声リングバッファ（30s上限・drop-oldest） (#7)"
```

---

## Task 5: リサンプラ（任意SR→16kHz mono f32）

**Files:**
- Create/Replace: `crates/shogun-core/src/audio/resample.rs`

- [ ] **Step 1: 失敗するテストを書く**

純ロジックの線形リサンプルで十分（ASR前処理として品質は許容範囲、依存を増やさない）。将来 rubato に差し替え可能。

`crates/shogun-core/src/audio/resample.rs`（stubを置換）:

```rust
//! Bring any capture stream to the one rate the VAD and ASR agree on: 16 kHz mono f32 (§3).
//!
//! Linear interpolation, deliberately dependency-free — ASR is robust to the mild aliasing this
//! introduces, and it keeps the pure-logic layer unit-testable on CI. Swap in a polyphase
//! resampler (rubato) later behind the same signature if measurement shows it matters.

use super::SAMPLE_RATE;

/// Downmix to mono by averaging interleaved channels.
pub fn to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    let ch = channels as usize;
    interleaved
        .chunks(ch)
        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
        .collect()
}

/// Resample mono `input` from `in_rate` to 16 kHz by linear interpolation.
pub fn to_16k_mono(input: &[f32], in_rate: u32) -> Vec<f32> {
    if in_rate == SAMPLE_RATE || input.is_empty() {
        return input.to_vec();
    }
    let ratio = SAMPLE_RATE as f64 / in_rate as f64;
    let out_len = ((input.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f64 / ratio;
        let j = src.floor() as usize;
        let frac = (src - j as f64) as f32;
        let a = input[j.min(input.len() - 1)];
        let b = input[(j + 1).min(input.len() - 1)];
        out.push(a + (b - a) * frac);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_passthrough() {
        assert_eq!(to_mono(&[1.0, 2.0, 3.0], 1), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn stereo_averages_to_mono() {
        // L,R,L,R → averaged
        assert_eq!(to_mono(&[0.0, 2.0, 4.0, 8.0], 2), vec![1.0, 6.0]);
    }

    #[test]
    fn same_rate_is_passthrough() {
        assert_eq!(to_16k_mono(&[1.0, 2.0], 16_000), vec![1.0, 2.0]);
    }

    #[test]
    fn downsampling_halves_length_from_32k() {
        let input = vec![0.5_f32; 320]; // 320 @ 32k → ~160 @ 16k
        let out = to_16k_mono(&input, 32_000);
        assert!((out.len() as i64 - 160).abs() <= 1, "unexpected length {}", out.len());
        assert!(out.iter().all(|&s| (s - 0.5).abs() < 1e-6));
    }
}
```

- [ ] **Step 2: テスト＋clippy＋コミット**

```bash
cargo test -p shogun-core audio::resample 2>&1 | tail -15   # PASS (4)
cargo clippy -p shogun-core --all-targets 2>&1 | tail -5
git add crates/shogun-core/src/audio/resample.rs
git commit -m "feat(core): 16kHz mono リサンプラ (#7)"
```

---

## Task 6: VAD（発話区間の切り出し）

**Files:**
- Create/Replace: `crates/shogun-core/src/audio/vad.rs`

- [ ] **Step 1: 失敗するテストを書く**

自前のエネルギーベースVAD。フレーム単位でRMSが閾値を超える区間を発話とし、無音がハングオーバ長続いたら発話を確定。最大長でも強制フラッシュ（リングの30s壁と対応）。

`crates/shogun-core/src/audio/vad.rs`（stubを置換）:

```rust
//! Cut a continuous stream into utterances at silence boundaries (§2).
//!
//! Energy-based and stateful: frames above an RMS floor are speech; once speech has been seen and
//! silence persists for `hangover`, the utterance is emitted. A speech run that reaches
//! `max_samples` is force-flushed so nothing can grow past the ring's 30s wall. Deliberately
//! simple and dependency-free so the boundary logic is exhaustively testable; a spectral VAD can
//! replace it behind the same `push`/`flush` shape.

use super::SAMPLE_RATE;

/// 20 ms frames at 16 kHz.
const FRAME: usize = SAMPLE_RATE as usize / 50;

pub struct Vad {
    rms_floor: f32,
    hangover_frames: usize,
    max_samples: usize,
    min_samples: usize,
    cur: Vec<f32>,
    in_speech: bool,
    silence_run: usize,
    pending_frame: Vec<f32>,
}

/// One completed utterance's samples, relative to the stream. The caller stamps the wall-clock
/// time and speaker.
pub type Cut = Vec<f32>;

impl Vad {
    /// Defaults tuned for meeting speech: ~ -40 dBFS floor, 500 ms hangover, 30 s max, 300 ms min.
    pub fn new() -> Self {
        Vad {
            rms_floor: 0.01,
            hangover_frames: 25, // 25 * 20ms = 500ms
            max_samples: 30 * SAMPLE_RATE as usize,
            min_samples: SAMPLE_RATE as usize * 300 / 1000,
            cur: Vec::new(),
            in_speech: false,
            silence_run: 0,
            pending_frame: Vec::new(),
        }
    }

    fn is_speech(frame: &[f32], floor: f32) -> bool {
        let sum_sq: f32 = frame.iter().map(|s| s * s).sum();
        (sum_sq / frame.len() as f32).sqrt() > floor
    }

    /// Feed samples; returns any utterances that completed within this chunk.
    pub fn push(&mut self, samples: &[f32]) -> Vec<Cut> {
        let mut out = Vec::new();
        self.pending_frame.extend_from_slice(samples);
        while self.pending_frame.len() >= FRAME {
            let frame: Vec<f32> = self.pending_frame.drain(..FRAME).collect();
            let speech = Self::is_speech(&frame, self.rms_floor);
            if speech {
                self.in_speech = true;
                self.silence_run = 0;
                self.cur.extend_from_slice(&frame);
            } else if self.in_speech {
                self.cur.extend_from_slice(&frame);
                self.silence_run += 1;
                if self.silence_run >= self.hangover_frames {
                    if let Some(c) = self.take() {
                        out.push(c);
                    }
                }
            }
            if self.cur.len() >= self.max_samples {
                if let Some(c) = self.take() {
                    out.push(c);
                }
            }
        }
        out
    }

    /// Emit whatever speech is buffered (used at stop). `None` if nothing usable.
    pub fn flush(&mut self) -> Option<Cut> {
        self.take()
    }

    fn take(&mut self) -> Option<Cut> {
        self.in_speech = false;
        self.silence_run = 0;
        if self.cur.len() >= self.min_samples {
            Some(std::mem::take(&mut self.cur))
        } else {
            self.cur.clear();
            None
        }
    }
}

impl Default for Vad {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(n: usize, amp: f32) -> Vec<f32> {
        (0..n).map(|i| amp * ((i as f32) * 0.3).sin()).collect()
    }

    #[test]
    fn silence_alone_yields_nothing() {
        let mut v = Vad::new();
        let cuts = v.push(&vec![0.0_f32; SAMPLE_RATE as usize]);
        assert!(cuts.is_empty());
        assert!(v.flush().is_none());
    }

    #[test]
    fn speech_then_silence_emits_one_utterance() {
        let mut v = Vad::new();
        let mut cuts = v.push(&tone(SAMPLE_RATE as usize, 0.3)); // 1s speech
        cuts.extend(v.push(&vec![0.0_f32; SAMPLE_RATE as usize])); // 1s silence > hangover
        assert_eq!(cuts.len(), 1, "expected exactly one utterance");
        assert!(cuts[0].len() >= SAMPLE_RATE as usize);
    }

    #[test]
    fn too_short_speech_is_dropped() {
        let mut v = Vad::new();
        let mut cuts = v.push(&tone(SAMPLE_RATE as usize / 20, 0.3)); // 50ms < 300ms min
        cuts.extend(v.push(&vec![0.0_f32; SAMPLE_RATE as usize]));
        assert!(cuts.is_empty());
    }

    #[test]
    fn force_flush_at_max_length() {
        let mut v = Vad::new();
        let cuts = v.push(&tone(31 * SAMPLE_RATE as usize, 0.3)); // 31s continuous
        assert!(!cuts.is_empty(), "a 31s run must be force-flushed at 30s");
        assert!(cuts[0].len() <= 30 * SAMPLE_RATE as usize + FRAME);
    }
}
```

- [ ] **Step 2: テスト＋clippy＋コミット**

```bash
cargo test -p shogun-core audio::vad 2>&1 | tail -15   # PASS (4)
cargo clippy -p shogun-core --all-targets 2>&1 | tail -5
git add crates/shogun-core/src/audio/vad.rs
git commit -m "feat(core): エネルギーVADで発話区間を切り出す (#7)"
```

---

## Task 7: Transcriber trait と FakeTranscriber

**Files:**
- Create: `crates/shogun-core/src/audio/asr/mod.rs`（Task 3 の stub `asr.rs` を `asr/mod.rs` に格上げ）

注意: Task 3 では `pub mod asr;` を宣言し stub を `audio/asr.rs` に置いた。ここで `audio/asr/mod.rs` へ移す（`git mv` 不要、`asr.rs` を削除し `asr/mod.rs` を作成）。

- [ ] **Step 1: trait と fake を書く**

`crates/shogun-core/src/audio/asr/mod.rs`:

```rust
//! The ASR seam. `worker` depends on this trait, not on whisper — so the pipeline is tested with a
//! deterministic fake, and the real engine (and a future Apple SpeechAnalyzer backend on macOS 26+)
//! plug in behind the same shape (§5).

#[cfg(all(feature = "audio", target_os = "macos"))]
pub mod whisper;

use super::Segment;

/// Turn 16 kHz mono f32 PCM into text. Given an in-memory slice — never a file path — so no caller
/// can be tempted to spill audio to disk (invariant 2).
pub trait Transcriber: Send {
    /// Transcribe one utterance. Returns zero or more lines. An empty result is normal (silence,
    /// or audio the model could not read) and must not be an error the caller has to handle.
    fn transcribe(&mut self, pcm: &[f32]) -> Vec<Segment>;
}

/// Deterministic stand-in for pipeline tests: emits one line whose text encodes the sample count,
/// so a test can assert the worker forwarded the right audio without a model.
#[derive(Default)]
pub struct FakeTranscriber {
    pub calls: usize,
}

impl Transcriber for FakeTranscriber {
    fn transcribe(&mut self, pcm: &[f32]) -> Vec<Segment> {
        self.calls += 1;
        if pcm.is_empty() {
            return Vec::new();
        }
        vec![Segment { text: format!("utterance-{}-samples", pcm.len()), confidence: 0.99 }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_emits_one_line_per_nonempty_utterance() {
        let mut t = FakeTranscriber::default();
        assert_eq!(t.transcribe(&[]).len(), 0);
        let out = t.transcribe(&[0.1, 0.2, 0.3]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text, "utterance-3-samples");
        assert_eq!(t.calls, 2);
    }
}
```

Task 3 で作った stub の `crates/shogun-core/src/audio/asr.rs` を削除。

- [ ] **Step 2: テスト＋コミット**

```bash
rm -f crates/shogun-core/src/audio/asr.rs
cargo test -p shogun-core audio::asr 2>&1 | tail -15   # PASS (1)
cargo clippy -p shogun-core --all-targets 2>&1 | tail -5
git add crates/shogun-core/src/audio/asr/
git rm --cached crates/shogun-core/src/audio/asr.rs 2>/dev/null || true
git commit -m "feat(core): Transcriber trait と FakeTranscriber (#7)"
```

---

## Task 8: AudioSource trait と FakeSource

**Files:**
- Create: `crates/shogun-core/src/audio/capture/mod.rs`（Task 3 の stub `capture.rs` を `capture/mod.rs` に格上げ）

- [ ] **Step 1: trait と fake を書く**

`crates/shogun-core/src/audio/capture/mod.rs`:

```rust
//! The capture seam. A source pushes 16 kHz mono f32 frames tagged with a speaker. The real
//! backends (`mic` via cpal, `system_tap` via Core Audio) live behind the `audio` feature and
//! macOS; the fake drives worker tests on CI.

#[cfg(all(feature = "audio", target_os = "macos"))]
pub mod mic;
#[cfg(all(feature = "audio", target_os = "macos"))]
pub mod system_tap;

use super::Speaker;

/// A frame of already-resampled 16 kHz mono audio from one source.
pub struct Frame {
    pub speaker: Speaker,
    pub samples: Vec<f32>,
}

/// A running capture source. `try_recv` is non-blocking so the worker can poll mic and tap on one
/// thread without either starving the other. `None` means "no data right now", not "closed".
pub trait AudioSource: Send {
    fn try_recv(&mut self) -> Option<Frame>;
    /// Stop capture and release the device. Idempotent.
    fn stop(&mut self);
}

/// A scripted source for tests: hands out pre-canned frames, then reports empty.
pub struct FakeSource {
    frames: std::collections::VecDeque<Frame>,
    pub stopped: bool,
}

impl FakeSource {
    pub fn new(frames: Vec<Frame>) -> Self {
        FakeSource { frames: frames.into(), stopped: false }
    }
}

impl AudioSource for FakeSource {
    fn try_recv(&mut self) -> Option<Frame> {
        self.frames.pop_front()
    }
    fn stop(&mut self) {
        self.stopped = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_yields_frames_then_empty() {
        let mut s = FakeSource::new(vec![Frame { speaker: Speaker::Me, samples: vec![0.1] }]);
        assert!(s.try_recv().is_some());
        assert!(s.try_recv().is_none());
        s.stop();
        assert!(s.stopped);
    }
}
```

Task 3 の stub `crates/shogun-core/src/audio/capture.rs` を削除。

- [ ] **Step 2: テスト＋コミット**

```bash
rm -f crates/shogun-core/src/audio/capture.rs
cargo test -p shogun-core audio::capture 2>&1 | tail -15   # PASS (1)
cargo clippy -p shogun-core --all-targets 2>&1 | tail -5
git add crates/shogun-core/src/audio/capture/
git commit -m "feat(core): AudioSource trait と FakeSource (#7)"
```

---

## Task 9: worker（capture+VAD+ASR を束ねる純ロジック）

worker は「1回の処理ステップ」を純関数的に扱えるよう設計する。実スレッド駆動は Task 12（配線）で薄く被せる。テストは fake source + fake transcriber + in-memory DB で決定論的に。

**Files:**
- Create/Replace: `crates/shogun-core/src/audio/worker.rs`
- Modify: `crates/shogun-core/Cargo.toml`（worker テストは `db` feature 下で shogun-memory を使う。テストのみ dev-dependency で shogun-memory を引く）

- [ ] **Step 1: Cargo に dev-dependency を追加**

`crates/shogun-core/Cargo.toml` の `[dev-dependencies]` に（無ければ節ごと追加）:

```toml
[dev-dependencies]
shogun-memory = { path = "../shogun-memory" }
```

（既に dev-dependencies がある場合はこの行を追加。重複させない。）

- [ ] **Step 2: worker を書く（sink は trait で抽象化しDB非依存にテスト）**

`crates/shogun-core/src/audio/worker.rs`（stub を置換）:

```rust
//! Ties capture → per-speaker VAD → ASR → sink into one pollable unit (§3).
//!
//! Kept sink-agnostic via `SegmentSink` so the pure pipeline is tested without a database, then
//! wired to `transcript_segments` at the call site. One `Vad` per speaker: mic and system audio
//! are independent streams and must not have their silence boundaries interleaved.

use super::asr::Transcriber;
use super::capture::{AudioSource, Frame};
use super::vad::Vad;
use super::{Speaker, Utterance};

/// Where finished lines go. The desktop implements this over `transcript_segments`; tests collect
/// into a Vec.
pub trait SegmentSink {
    fn emit(&mut self, u: &Utterance, text: &str, confidence: f64);
}

pub struct Worker<S: AudioSource, T: Transcriber> {
    source: S,
    asr: T,
    vad_me: Vad,
    vad_other: Vad,
    /// Wall-clock ms of the current step, set by the driver each poll.
    now: i64,
}

impl<S: AudioSource, T: Transcriber> Worker<S, T> {
    pub fn new(source: S, asr: T) -> Self {
        Worker { source, asr, vad_me: Vad::new(), vad_other: Vad::new(), now: 0 }
    }

    fn transcribe_cut(&mut self, speaker: Speaker, pcm: Vec<f32>, sink: &mut dyn SegmentSink) {
        let u = Utterance { speaker, started_at: self.now, pcm };
        for seg in self.asr.transcribe(&u.pcm) {
            let text = seg.text.trim();
            if !text.is_empty() {
                sink.emit(&u, text, seg.confidence);
            }
        }
        // u (and its pcm) is dropped here — the waveform never outlives transcription.
    }

    /// Drain all currently-available frames, transcribing any utterances they complete. Returns the
    /// number of frames consumed (0 means idle). Non-blocking.
    pub fn poll(&mut self, now: i64, sink: &mut dyn SegmentSink) -> usize {
        self.now = now;
        let mut consumed = 0;
        while let Some(Frame { speaker, samples }) = self.source.try_recv() {
            consumed += 1;
            let vad = match speaker {
                Speaker::Me => &mut self.vad_me,
                Speaker::Other => &mut self.vad_other,
            };
            for cut in vad.push(&samples) {
                self.transcribe_cut(speaker, cut, sink);
            }
        }
        consumed
    }

    /// Stop capture and flush the final utterance on each speaker before the buffers are dropped.
    pub fn stop(&mut self, now: i64, sink: &mut dyn SegmentSink) {
        self.now = now;
        self.source.stop();
        if let Some(cut) = self.vad_me.flush() {
            self.transcribe_cut(Speaker::Me, cut, sink);
        }
        if let Some(cut) = self.vad_other.flush() {
            self.transcribe_cut(Speaker::Other, cut, sink);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::asr::FakeTranscriber;
    use crate::audio::capture::FakeSource;
    use crate::audio::SAMPLE_RATE;

    #[derive(Default)]
    struct VecSink {
        lines: Vec<(Speaker, String, f64)>,
    }
    impl SegmentSink for VecSink {
        fn emit(&mut self, u: &Utterance, text: &str, confidence: f64) {
            self.lines.push((u.speaker, text.to_string(), confidence));
        }
    }

    fn tone(n: usize) -> Vec<f32> {
        (0..n).map(|i| 0.3 * ((i as f32) * 0.3).sin()).collect()
    }

    #[test]
    fn speech_then_silence_produces_a_line_via_asr() {
        let frames = vec![
            Frame { speaker: Speaker::Me, samples: tone(SAMPLE_RATE as usize) },
            Frame { speaker: Speaker::Me, samples: vec![0.0; SAMPLE_RATE as usize] },
        ];
        let mut w = Worker::new(FakeSource::new(frames), FakeTranscriber::default());
        let mut sink = VecSink::default();
        w.poll(1_000, &mut sink);
        assert_eq!(sink.lines.len(), 1);
        assert_eq!(sink.lines[0].0, Speaker::Me);
        assert!(sink.lines[0].1.starts_with("utterance-"));
    }

    #[test]
    fn mic_and_system_are_segmented_independently() {
        let frames = vec![
            Frame { speaker: Speaker::Me, samples: tone(SAMPLE_RATE as usize) },
            Frame { speaker: Speaker::Other, samples: tone(SAMPLE_RATE as usize) },
            Frame { speaker: Speaker::Me, samples: vec![0.0; SAMPLE_RATE as usize] },
            Frame { speaker: Speaker::Other, samples: vec![0.0; SAMPLE_RATE as usize] },
        ];
        let mut w = Worker::new(FakeSource::new(frames), FakeTranscriber::default());
        let mut sink = VecSink::default();
        w.poll(1_000, &mut sink);
        let speakers: Vec<Speaker> = sink.lines.iter().map(|l| l.0).collect();
        assert!(speakers.contains(&Speaker::Me) && speakers.contains(&Speaker::Other));
    }

    #[test]
    fn stop_flushes_trailing_speech() {
        let frames = vec![Frame { speaker: Speaker::Me, samples: tone(SAMPLE_RATE as usize) }];
        let mut w = Worker::new(FakeSource::new(frames), FakeTranscriber::default());
        let mut sink = VecSink::default();
        w.poll(1_000, &mut sink); // no trailing silence yet → nothing emitted
        assert_eq!(sink.lines.len(), 0);
        w.stop(2_000, &mut sink); // flush emits the buffered utterance
        assert_eq!(sink.lines.len(), 1);
    }
}
```

- [ ] **Step 3: テスト＋clippy＋コミット**

```bash
cargo test -p shogun-core audio::worker 2>&1 | tail -20   # PASS (3)
cargo clippy -p shogun-core --all-targets 2>&1 | tail -5
git add crates/shogun-core/src/audio/worker.rs crates/shogun-core/Cargo.toml
git commit -m "feat(core): 音声worker（capture→VAD→ASR→sink） (#7)"
```

---

## Task 10: whisper-rs 実装（feature `audio`, macOS）

このタスクは実 FFI。ユニットテストではなく `#[ignore]` のゴールデンと手動確認で担保する（デバイス/モデル依存）。

**Files:**
- Create/Replace: `crates/shogun-core/src/audio/asr/whisper.rs`
- Modify: `crates/shogun-core/Cargo.toml`（`audio` feature と whisper-rs 追加）

- [ ] **Step 1: Cargo に feature と依存を追加**

`crates/shogun-core/Cargo.toml` の `[features]` に:

```toml
# The MT3 audio lane: the real capture + ASR backends. macOS-only heavy deps (whisper.cpp/Metal,
# cpal, Core Audio via objc2), so OFF by default — the pure-logic audio tests compile without them.
audio = ["dep:whisper-rs", "dep:cpal"]
```

`[dependencies]` に（optional）:

```toml
# On-device ASR: whisper.cpp with Metal. Fed in-memory f32 PCM (no file path), so invariant 2 holds.
whisper-rs = { version = "0.12", optional = true }
# Microphone capture.
cpal = { version = "0.15", optional = true }
```

（objc2 / core-audio-sys は Task 11 の system_tap で追加。まず mic + whisper を通す。）

- [ ] **Step 2: whisper ラッパを書く**

`crates/shogun-core/src/audio/asr/whisper.rs`:

```rust
//! whisper.cpp backend (whisper-rs, Metal). Fed an in-memory f32 slice — never a file — so the
//! waveform never touches disk (invariant 2). small is the bundled default; large-v3-turbo is the
//! opt-in high-accuracy model (§5). Language is auto-detected per utterance.

use super::super::Segment;
use super::Transcriber;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct Whisper {
    ctx: WhisperContext,
}

impl Whisper {
    /// Load a gguf model from `model_path`. Errors if the file is missing/corrupt — the caller
    /// degrades the audio lane to off rather than failing the meeting (see meeting.rs wiring).
    pub fn load(model_path: &str) -> Result<Self, String> {
        let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
            .map_err(|e| format!("whisper load failed: {e}"))?;
        Ok(Whisper { ctx })
    }
}

impl Transcriber for Whisper {
    fn transcribe(&mut self, pcm: &[f32]) -> Vec<Segment> {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(None); // auto-detect (§5)
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_no_context(true); // each utterance is independent; avoids cross-talk carryover

        let Ok(mut state) = self.ctx.create_state() else {
            return Vec::new();
        };
        if state.full(params, pcm).is_err() {
            return Vec::new();
        }
        let n = state.full_n_segments().unwrap_or(0);
        let mut out = Vec::new();
        for i in 0..n {
            let Ok(text) = state.full_get_segment_text(i) else { continue };
            let text = text.trim().to_string();
            if text.is_empty() {
                continue;
            }
            // Map the segment's mean token logprob into [0,1]. whisper-rs exposes token probs; a
            // simple, monotone proxy is exp(mean_logprob) clamped. Kept conservative.
            let conf = segment_confidence(&mut state, i);
            out.push(Segment { text, confidence: conf });
        }
        out
    }
}

/// mean token probability of segment `i`, in [0,1]. Falls back to 0.5 when probs are unavailable.
fn segment_confidence(state: &mut whisper_rs::WhisperState, i: i32) -> f64 {
    let tokens = state.full_n_tokens(i).unwrap_or(0);
    if tokens == 0 {
        return 0.5;
    }
    let mut sum = 0.0_f64;
    let mut count = 0;
    for t in 0..tokens {
        if let Ok(p) = state.full_get_token_prob(i, t) {
            sum += p as f64;
            count += 1;
        }
    }
    if count == 0 {
        0.5
    } else {
        (sum / count as f64).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden: requires a bundled small model at $SHOGUN_WHISPER_MODEL and a tiny PCM fixture.
    /// Ignored by default (heavy, model-gated); run in CI with the model cached:
    ///   SHOGUN_WHISPER_MODEL=... cargo test -p shogun-core --features audio -- --ignored whisper_golden
    #[test]
    #[ignore]
    fn whisper_golden_transcribes_english() {
        let model = std::env::var("SHOGUN_WHISPER_MODEL").expect("set SHOGUN_WHISPER_MODEL");
        let mut w = Whisper::load(&model).expect("load");
        // 16k mono f32 of a short spoken phrase, generated or licensed — never user audio.
        let pcm = load_fixture_pcm("tests/fixtures/hello_16k.f32");
        let segs = w.transcribe(&pcm);
        assert!(!segs.is_empty(), "expected a transcript for spoken audio");
    }

    #[allow(dead_code)]
    fn load_fixture_pcm(path: &str) -> Vec<f32> {
        let bytes = std::fs::read(path).expect("fixture");
        bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect()
    }
}
```

- [ ] **Step 3: feature 有効でコンパイルすることを確認**

Run: `cargo build -p shogun-core --features audio 2>&1 | tail -30`
Expected: PASS。whisper-rs のAPI名（`full_get_token_prob` 等）はバージョンで差異があり得るため、コンパイルエラーが出たら whisper-rs 0.12 の実APIに合わせて修正（`cargo doc -p whisper-rs --open` で確認）。**ここは実APIに合わせる調整が入る前提のタスク。**

- [ ] **Step 4: 既存の純ロジックテストが feature OFF でも通ることを確認**

Run: `cargo test -p shogun-core audio 2>&1 | tail -15`
Expected: PASS（feature OFF なので whisper モジュールはコンパイルされない）

- [ ] **Step 5: コミット**

```bash
git add crates/shogun-core/src/audio/asr/whisper.rs crates/shogun-core/Cargo.toml
git commit -m "feat(core): whisper.cpp オンデバイスASRバックエンド (#7)"
```

---

## Task 11: マイク（cpal）と system tap（Core Audio）バックエンド

実 FFI。手動確認で担保。

**Files:**
- Create/Replace: `crates/shogun-core/src/audio/capture/mic.rs`
- Create/Replace: `crates/shogun-core/src/audio/capture/system_tap.rs`
- Modify: `crates/shogun-core/Cargo.toml`（objc2 / core-audio-sys 追加、`audio` feature に紐付け）

- [ ] **Step 1: cpal マイクを書く**

`crates/shogun-core/src/audio/capture/mic.rs`:

```rust
//! Microphone capture via cpal. Speaker = `Me`. Runs cpal's callback on its own thread and hands
//! resampled 16 kHz mono frames to the worker over a channel; the worker polls `try_recv`.

use super::super::{resample, Speaker};
use super::{AudioSource, Frame};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc::{Receiver, TryRecvError};

pub struct Mic {
    rx: Receiver<Vec<f32>>,
    _stream: cpal::Stream,
}

impl Mic {
    /// Open the default input device. Err (permission denied, no device) → caller degrades to
    /// notes-only (meeting.rs), never crashes.
    pub fn open() -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or("no input device")?;
        let config = device.default_input_config().map_err(|e| e.to_string())?;
        let in_rate = config.sample_rate().0;
        let channels = config.channels();
        let (tx, rx) = std::sync::mpsc::channel();
        let err_fn = |e| eprintln!("[meeting] mic stream error: {e}");
        let stream = device
            .build_input_stream(
                &config.into(),
                move |data: &[f32], _| {
                    let mono = resample::to_mono(data, channels);
                    let f16k = resample::to_16k_mono(&mono, in_rate);
                    let _ = tx.send(f16k);
                },
                err_fn,
                None,
            )
            .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Mic { rx, _stream: stream })
    }
}

impl AudioSource for Mic {
    fn try_recv(&mut self) -> Option<Frame> {
        match self.rx.try_recv() {
            Ok(samples) => Some(Frame { speaker: Speaker::Me, samples }),
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
    }
    fn stop(&mut self) {
        // Dropping `_stream` on Drop stops cpal; nothing to do explicitly.
    }
}
```

- [ ] **Step 2: system tap を書く（14.4+ ゲート）**

`crates/shogun-core/src/audio/capture/system_tap.rs`:

```rust
//! System audio via a Core Audio process tap (`CATapDescription` / `AudioHardwareCreateProcessTap`,
//! macOS 14.4+). Speaker = `Other`. This is how the other side of a meeting is captured without a
//! bot joining the call (Issue #7 Non-Goal).
//!
//! On macOS 14.0–14.3 the tap API does not exist, so `open` returns `Ok(None)` and the lane runs
//! mic-only (§7). A TCC denial also returns `None`. Never a hard error — the meeting still records.

use super::super::Speaker;
use super::{AudioSource, Frame};
use std::sync::mpsc::{Receiver, TryRecvError};

pub struct SystemTap {
    rx: Receiver<Vec<f32>>,
}

impl SystemTap {
    /// `Ok(None)` = not available on this OS / permission (degrade to mic-only). `Ok(Some)` = tap
    /// running. `Err` is reserved for genuinely unexpected failures the caller logs once.
    pub fn open() -> Result<Option<Self>, String> {
        if !process_tap_supported() {
            eprintln!("[meeting] system audio tap unavailable (needs macOS 14.4+); mic only");
            return Ok(None);
        }
        // NOTE: the objc2 / core-audio-sys plumbing for CATapDescription +
        // AudioHardwareCreateProcessTap + an aggregate device tapping it is implemented here and
        // pushes resampled 16k mono frames onto `tx`. It is device-level FFI and is verified on a
        // real machine (Task 13), not in unit tests. Kept behind the OS check above so 14.0–14.3
        // never reaches it.
        create_tap_stream()
    }
}

/// True on macOS 14.4+. Reads the OS version via ProcessInfo.
fn process_tap_supported() -> bool {
    // objc2-foundation: NSProcessInfo.operatingSystemVersion >= 14.4
    // (implementation detail; see Task 13 verification).
    macos_at_least(14, 4)
}

impl AudioSource for SystemTap {
    fn try_recv(&mut self) -> Option<Frame> {
        match self.rx.try_recv() {
            Ok(samples) => Some(Frame { speaker: Speaker::Other, samples }),
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
    }
    fn stop(&mut self) {
        // Aggregate device + tap are torn down on Drop.
    }
}

// --- FFI helpers, implemented against core-audio-sys / objc2 and verified on device (Task 13) ---

fn macos_at_least(major: i64, minor: i64) -> bool {
    use objc2_foundation::NSProcessInfo;
    let v = NSProcessInfo::processInfo().operatingSystemVersion();
    (v.majorVersion, v.minorVersion) >= (major, minor)
}

fn create_tap_stream() -> Result<Option<SystemTap>, String> {
    // Implemented in Task 13 against the real API; returns Ok(Some(SystemTap{rx})) on success,
    // Ok(None) on TCC denial. Placeholder compile-guard until then:
    Err("system tap FFI not yet wired (Task 13)".into())
}
```

**注意（正直に）:** Core Audio process tap の FFI は device 上でしか検証できない。Step 2 の `create_tap_stream`/`macos_at_least` は実 API（`core-audio-sys` の `AudioHardwareCreateProcessTap`、`objc2-foundation` の `NSProcessInfo`）に合わせて Task 13 の実機作業で確定する。それまでは `audio` feature を有効にしても `SystemTap::open()` は `Err` を返し、呼び出し側（Task 12）が mic-only に縮退する。

- [ ] **Step 3: Cargo に FFI 依存を追加**

`crates/shogun-core/Cargo.toml` の `audio` feature を更新:

```toml
audio = ["dep:whisper-rs", "dep:cpal", "dep:objc2-foundation", "dep:core-audio-sys"]
```

`[dependencies]`:

```toml
objc2-foundation = { version = "0.2", optional = true }
core-audio-sys = { version = "0.2", optional = true }
```

- [ ] **Step 4: コンパイル確認**

Run: `cargo build -p shogun-core --features audio 2>&1 | tail -30`
Expected: PASS（mic は完全、system_tap は Err スタブで型は満たす）

- [ ] **Step 5: コミット**

```bash
git add crates/shogun-core/src/audio/capture/mic.rs crates/shogun-core/src/audio/capture/system_tap.rs crates/shogun-core/Cargo.toml
git commit -m "feat(core): マイク(cpal)取得とsystem tapの骨格 (#7)"
```

---

## Task 12: 設定に asr_model を追加

**Files:**
- Modify: `crates/shogun-core/src/meeting/settings.rs`

- [ ] **Step 1: 現状を読む**

Run: `sed -n '1,80p' crates/shogun-core/src/meeting/settings.rs`
既存の `Settings` 構造体（serde derive、既定OFF）の形に合わせる。

- [ ] **Step 2: asr_model フィールドとテストを追加**

`Settings` に以下を追加（既存フィールドの並びと serde default に合わせる）:

```rust
/// Which on-device ASR model to use. `Small` (bundled default) or `Turbo` (large-v3-turbo,
/// opt-in high accuracy, fetched on first use). Defaults to Small (§5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AsrModel {
    #[default]
    Small,
    Turbo,
}
```

`Settings` 構造体本体にフィールド追加:

```rust
    #[serde(default)]
    pub asr_model: AsrModel,
```

テスト（同ファイルの `#[cfg(test)] mod tests` に追加）:

```rust
    #[test]
    fn asr_model_defaults_to_small() {
        assert_eq!(AsrModel::default(), AsrModel::Small);
    }

    #[test]
    fn asr_model_round_trips_json() {
        let json = serde_json::to_string(&AsrModel::Turbo).unwrap();
        assert_eq!(json, "\"turbo\"");
        let back: AsrModel = serde_json::from_str(&json).unwrap();
        assert_eq!(back, AsrModel::Turbo);
    }
```

- [ ] **Step 3: テスト＋コミット**

```bash
cargo test -p shogun-core meeting::settings 2>&1 | tail -15   # PASS
cargo clippy -p shogun-core --all-targets 2>&1 | tail -5
git add crates/shogun-core/src/meeting/settings.rs
git commit -m "feat(core): 会議ノートのASRモデル設定（small既定/turbo） (#7)"
```

---

## Task 13: desktop 配線（StartAudio/StopAudio → worker）と実機検証

**Files:**
- Modify: `apps/desktop/src-tauri/src/meeting.rs`（`apply()` の `Effect::StartAudio | Effect::StopAudio => {}`）
- Modify: `apps/desktop/src-tauri/Cargo.toml`（shogun-core の `audio` feature を有効化）
- Create: `crates/shogun-core/src/audio/mod.rs` に DB sink 実装は置かず、desktop 側に `TranscriptSink`

- [ ] **Step 1: desktop 側 sink と worker 保持を実装**

`apps/desktop/src-tauri/src/meeting.rs` に、`Lane` へ worker ハンドル（停止用）を持たせ、`StartAudio` で起動スレッドを spawn、`StopAudio` で停止する。sink は `transcript_segments::append` を呼ぶ。

`Effect::StartAudio | Effect::StopAudio => {}` を以下に置換:

```rust
                Effect::StartAudio => {
                    if let Some(id) = lane.session_id {
                        audio_lane::start(app, lane, id);
                    }
                }
                Effect::StopAudio => {
                    audio_lane::stop(lane);
                }
```

新規モジュール `apps/desktop/src-tauri/src/audio_lane.rs`（driver）:

```rust
//! Drives the core audio Worker on a background thread for the life of a Recording interval, and
//! writes each finished line to `transcript_segments`. Degrades to notes-only if the mic or model
//! cannot be opened — a meeting never fails because audio did (§7).

use shogun_core::audio::asr::whisper::Whisper;
use shogun_core::audio::capture::mic::Mic;
use shogun_core::audio::capture::system_tap::SystemTap;
use shogun_core::audio::worker::{SegmentSink, Worker};
use shogun_core::audio::{Speaker, Utterance};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct Handle {
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

/// A sink that writes to the DB. Maps Speaker → the memory crate's Speaker.
struct DbSink<'a> {
    db: &'a shogun_core::daemon::Db,
    session_id: i64,
}

impl SegmentSink for DbSink<'_> {
    fn emit(&mut self, u: &Utterance, text: &str, confidence: f64) {
        use shogun_memory::transcript_segments::{append, NewSegment, Speaker as MSpeaker};
        let speaker = match u.speaker {
            Speaker::Me => MSpeaker::Me,
            Speaker::Other => MSpeaker::Other,
        };
        self.db.with_conn(|conn| {
            let _ = append(
                conn,
                &NewSegment { session_id: self.session_id, ts: u.started_at, speaker, text, confidence },
                u.started_at,
            );
        });
    }
}
```

**注意:** `daemon::Db` の実 API（`with_conn` 等）は既存コードに合わせる。`open_session`/`close_session` が使っている `db.open_meeting()` と同じ `Db` 型なので、そのメソッド群を読んで sink の書込経路を合わせること（Step 2）。

- [ ] **Step 2: Db の書込 API を確認して sink を合わせる**

Run: `grep -n "pub fn\|impl Db\|with_conn\|open_meeting\|conn" crates/shogun-core/src/daemon.rs | head -40`
`DbSink` を実際の `Db` の書込メソッドに合わせて修正（`with_conn` が無ければ、`open_meeting` と同様のラッパを1つ足す：`transcript_segments::append` を呼ぶ薄い `Db::append_transcript(...)` を daemon.rs に追加するのが素直）。

- [ ] **Step 3: start/stop を実装**

`audio_lane.rs` に追加:

```rust
pub fn start(app: &tauri::AppHandle, lane: &mut Lane, session_id: i64) {
    let Some(db) = super::meeting::db_handle(app) else { return }; // notes-only if no DB
    // Model path: bundled small, or fetched turbo per settings.
    let model_path = match super::meeting::whisper_model_path(app, lane) {
        Some(p) => p,
        None => { eprintln!("[meeting] no ASR model; notes only"); return; }
    };
    let asr = match Whisper::load(&model_path) {
        Ok(a) => a,
        Err(e) => { eprintln!("[meeting] {e}; notes only"); return; }
    };
    let mic = match Mic::open() {
        Ok(m) => m,
        Err(e) => { eprintln!("[meeting] mic unavailable ({e}); notes only"); return; }
    };
    // System tap is best-effort: absent on <14.4 or if TCC denied → mic-only.
    let tap = SystemTap::open().ok().flatten();

    let stop = Arc::new(AtomicBool::new(false));
    let stop_t = stop.clone();
    let db2 = db.clone();
    let join = std::thread::spawn(move || {
        let mut sink = DbSink { db: &db2, session_id };
        let mut worker = Worker::new(super::audio_lane_source::combine(mic, tap), asr);
        while !stop_t.load(Ordering::Relaxed) {
            let now = now_ms();
            if worker.poll(now, &mut sink) == 0 {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
        worker.stop(now_ms(), &mut sink);
    });
    lane.audio = Some(Handle { stop, join: Some(join) });
}

pub fn stop(lane: &mut Lane) {
    if let Some(mut h) = lane.audio.take() {
        h.stop.store(true, Ordering::Relaxed);
        if let Some(j) = h.join.take() {
            let _ = j.join();
        }
    }
}
```

**注意:** `Worker` は単一 `AudioSource` を取る設計。mic と tap の2ソースを1つに束ねる薄い `CombinedSource`（両方を `try_recv` でラウンドロビン）を `apps/desktop/src-tauri/src/audio_lane_source.rs` に実装するか、`Worker` を `Vec<Box<dyn AudioSource>>` を取る形に一般化する。**推奨: core 側の `Worker::new` を複数ソース対応に一般化**（`sources: Vec<Box<dyn AudioSource>>`）。その場合 Task 9 の `Worker` を見直し、`poll` で全ソースを回す。ここは実装時に core を1箇所調整する。

- [ ] **Step 4: Lane に audio フィールド、db_handle/whisper_model_path ヘルパを追加**

`Lane` 構造体に `audio: Option<audio_lane::Handle>` を追加。`db_handle`（`Db` を clone して返す）、`whisper_model_path`（バンドル small のパス、設定が Turbo ならフェッチ済みパス）を `meeting.rs` に実装。モデルフェッチ未実装時は small 固定でよい（turbo フェッチは §11 のオープン事項、別コミットで可）。

- [ ] **Step 5: desktop の Cargo で audio feature を有効化**

`apps/desktop/src-tauri/Cargo.toml` の shogun-core 依存に `"audio"` を追加:

```toml
shogun-core = { path = "../../../crates/shogun-core", features = ["exec", "audio"] }
```
（既存の features 指定に `audio` を足す。既存指定は開いて確認。）

- [ ] **Step 6: ビルド確認**

Run: `cargo build -p shogun-desktop --features ... 2>&1 | tail -30`（desktop のビルドコマンドは既存の手順に従う。`apps/desktop` の README/package.json のビルドスクリプトを確認。）
Expected: PASS

- [ ] **Step 7: 実機検証（手動・§7.3）**

macOS 14.4+ 実機で：
1. Zoom または Google Meet を開始 → フローティングパネルの「Note を取る」→ Start。
2. 数分話す（英語と日本語）。
3. Stop 後、DB を確認：
   Run: `sqlite3 <db> "SELECT ts, speaker, substr(text,1,40), confidence FROM transcript_segments ORDER BY ts DESC LIMIT 10;"`
   Expected: 自分の発話が `speaker='me'`、相手が `speaker='other'`（14.4+）で入っている。
4. `find <appdata> -name '*.wav' -o -name '*.pcm' -o -name '*.caf'` で**音声ファイルが1つも生成されていない**ことを確認（不変条件2）。
5. macOS 14.0–14.3 環境（あれば）で mic-only 縮退を確認。

- [ ] **Step 8: コミット**

```bash
git add apps/desktop/src-tauri/src/audio_lane.rs apps/desktop/src-tauri/src/meeting.rs apps/desktop/src-tauri/Cargo.toml crates/shogun-core/src/audio/worker.rs crates/shogun-core/src/daemon.rs
git commit -m "feat(desktop): 音声レーンを StartAudio/StopAudio に配線 (#7)"
```

---

## Task 14: ロールバック手順と進捗ドキュメント更新

**Files:**
- Create: `docs/migrations/V9-rollback.md`
- Modify: `docs/meeting-notes-ui-design.md`（§7.1 の進捗表）

- [ ] **Step 1: ロールバック手順を書く**

`docs/migrations/V9-rollback.md`（既存 V7/V8-rollback.md の書式に合わせる。まず `cat docs/migrations/V8-rollback.md` で書式確認）:

```markdown
# V9 ロールバック — transcript_segments

V9 は additive（1テーブル＋1インデックス追加）。前方互換のため通常はロールバック不要。

手動で戻す場合:

    DROP INDEX IF EXISTS idx_transcript_session;
    DROP TABLE IF EXISTS transcript_segments;
    DELETE FROM refinery_schema_history WHERE version = 9;

注意: 破棄されるのは文字起こしテキストのみ。音声は元々どこにも保存されていない（不変条件2）。
```

- [ ] **Step 2: 進捗表を更新**

`docs/meeting-notes-ui-design.md` §7.1 の MT3 行を更新:

```markdown
| **MT3** | ✅ 完了（配線まで到達） | オンデバイスASR（whisper.cpp small / whisper-rs・Metal）。mic=me / system tap(14.4+)=other、VADで発話区間を切り出し `transcript_segments` に保存。<14.4 はマイクのみに縮退。ライブ字幕は非表示のまま |
```

§8 未決事項の 1・2 を「決定済み（2026-07-28）」に更新し、残りオープン事項（VAD閾値の実機チューニング、turboフェッチ、confidence正規化式）を §11 として残す旨を追記。

- [ ] **Step 3: コミット**

```bash
git add docs/migrations/V9-rollback.md docs/meeting-notes-ui-design.md
git commit -m "docs(meeting): MT3完了を進捗表に反映＋V9ロールバック手順 (#7)"
```

---

## 完了条件

- [ ] `cargo test -p shogun-memory` 緑（transcript_segments 3テスト含む）
- [ ] `cargo test -p shogun-core audio` 緑（ring/resample/vad/asr/capture/worker、feature OFF）
- [ ] `cargo build -p shogun-core --features audio` 成功
- [ ] `cargo clippy --all-targets` warnings なし
- [ ] 実機でマイク＋（14.4+なら）システム音声の文字起こしが transcript_segments に入る
- [ ] 音声ファイルが1つも生成されない（不変条件2）
- [ ] §7.1 進捗表の MT3 が実態に整合（「配線まで到達」を確認してから✅）

## オープン事項（実装中に確定）

1. whisper-rs 0.12 の実API名（confidence 取得メソッド）— Task 10 でコンパイルに合わせる。
2. Core Audio process tap の FFI 詳細 — Task 11/13 で実機に合わせる。
3. `Worker` の複数ソース対応 — Task 13 Step 3 で core を一般化。
4. VAD 閾値・ハングオーバの実機チューニング。
5. turbo モデルのフェッチ・ハッシュ検証。

---

# 追補（2026-07-28）: 相手音声の tap FFI と残りオープン事項

MT3 の初回実装で `create_tap_stream` はスタブ（`Err`）のまま出荷したため、現状は mic-only。
オーナー指示により (1) tap FFI を実装、(3) 残りオープン事項を進める。

## Task 15: Core Audio process tap FFI（相手音声＝Speaker::Other）

**Files:**
- Modify: `crates/shogun-core/src/audio/capture/system_tap.rs`（`create_tap_stream` を実装）
- Modify: `crates/shogun-core/Cargo.toml`（`objc2-core-audio` + `objc2-core-audio-types` を `audio` feature に追加）

**アプローチ（macOS 14.4+ / `insidegui/AudioCap` リファレンスに準拠）:**
1. `CATapDescription`（`init(stereoGlobalTapButExcludeProcesses: [])` 相当＝全システム音、自プロセス除外）を objc2 で生成。private / mute-behavior=unmuted / name を設定。
2. `AudioHardwareCreateProcessTap(&desc, &mut tap_id)` でタップobjectを作成（`objc2-core-audio`）。
3. タップUIDを含む集約デバイスを `AudioHardwareCreateAggregateDevice` で作成（sub-device にタップUID）。
4. 集約デバイスに `AudioDeviceCreateIOProcIDWithBlock` で IOProc を設置し、`AudioDeviceStart`。IOProc 内で受け取った f32 バッファを `resample::to_mono`→`to_16k_mono` して `tx.send()`。IOProc からは重い処理をせずチャネルに流すだけ。
5. `SystemTap::stop`/`Drop` で `AudioDeviceStop`＋IOProc除去＋`AudioHardwareDestroyAggregateDevice`＋`AudioHardwareDestroyProcessTap`。
6. TCC 権限拒否や作成失敗は `Ok(None)`（mic-only 縮退）。14.4+ ゲート（`macos_at_least(14,4)`）は維持。
7. 不変条件2：バッファは RAM のチャネルのみ。ファイル化しない。

**検証:**
- `cargo build -p shogun-core --features audio` 成功、`--features audio` clippy 無警告。
- feature OFF の 19 テスト維持。
- 実機検証（実 Zoom/Meet で Other 行が入るか、TCC プロンプト、デバイスの解放）は**ユーザー環境**で行う（本タスクは compiles-and-links まで）。IOProc のスレッド安全性・`stop` が即座に返ることを実機で確認。

**エスカレーション:** `objc2-core-audio` に必要な関数（`AudioHardwareCreateProcessTap` 等）が無い/シグネチャが違う場合は、`extern "C"` 直接宣言＋`objc2` の `extern_class!` で `CATapDescription` を最小定義して補う。`flexaudio-os-macos` クレートで代替できるなら検討可（ただし依存の重さと RAM-only 制約を確認）。詰まったら BLOCKED で具体エラーを報告。

**Commit:** `feat(core): Core Audio process tap で相手音声を取得 (#7)`

## Task 16: 残りオープン事項

**16a. turbo モデルの初回フェッチ（settings.asr_model == Turbo 時）**
- Files: `apps/desktop/src-tauri/src/audio_lane.rs`（モデルパス選択）＋新規 `apps/desktop/src-tauri/src/model_fetch.rs`。
- 設定が Turbo のとき、`app_data_dir()/models/whisper-large-v3-turbo-q5_0.gguf` が無ければ HuggingFace `ggml-org/whisper.cpp`（または ggerganov ミラー）から**ピン留めSHA256付き**でダウンロード→検証→保存。検証失敗は破棄して small にフォールバック。ダウンロードは `net` feature の reqwest blocking を利用（無ければ ureq を検討）。進捗はログのみ。
- `whisper_model_path` を「Turbo かつ取得済みなら turbo、なければ bundled small」に変更。
- **不変条件**: モデルは静的アセット（ユーザー音声でない）。Keychain 対象外。
- Commit: `feat(desktop): turboモデルの初回フェッチとsettings連動 (#7)`

**16b. VAD パラメータの露出＋config化**
- Files: `crates/shogun-core/src/audio/vad.rs`（`Vad::with_params{rms_floor, hangover_ms, min_ms, max_ms}` を追加、`new()` は既定値でそれを呼ぶ）＋ `meeting/settings.rs`（任意: `vad_sensitivity` を low/med/high で持ち、med=既定）。
- 既存テストは `new()` 既定で不変。`with_params` の単体テスト追加。
- Commit: `feat(core): VADパラメータを設定可能にする (#7)`

**16c. confidence 正規化の確認と文書化**
- 現状は「セグメントの平均トークン確率を [0,1] にクランプ」。whisper-rs 0.16 の `token_probability()` は既に [0,1] なので妥当。special/timestamp トークンを平均から除外できるなら除外し、`whisper.rs` の doc コメントに正規化の定義を明記。式変更が不要なら「as-is で妥当」と設計書§11に追記。
- Commit: `docs+refactor(core): confidence正規化を明文化 (#7)`

**検証（Task 16 全体）:** 該当クレートの test + clippy 緑、desktop ビルド緑。

---

# 追補2（2026-07-28）: MT4 議事録生成（要約＋決定＋Next Action / 生成＋保存まで）

オーナー決定: 中身=要約+決定+Next Action / スコープ=生成+保存まで（UIカードは最小）。
要約は不変条件5どおり **Select KK Batch**（`llm::AnthropicBatchClient`）。会話チャンク送信は
既存トレース（`submit` が item 毎に記録）に乗る。Batch は非同期（数分）なので、MT2 の縮退
Recap を即表示し、本要約が届いたら差し替え。

## Slice 1（決定論・TDD・完全検証可能）

### V10 マイグレーション + リポジトリ
- `crates/shogun-memory/src/migrations/V10__meeting_recaps.sql`:
  ```sql
  CREATE TABLE meeting_recaps (
    id INTEGER PRIMARY KEY,
    session_id INTEGER NOT NULL UNIQUE REFERENCES sessions (id),
    summary TEXT NOT NULL,
    decisions TEXT NOT NULL,     -- JSON配列（文字列）
    next_actions TEXT NOT NULL,  -- JSON配列（{text, owner?}）
    model TEXT NOT NULL,         -- 生成モデル名（provenance）
    created_at INTEGER NOT NULL
  ) STRICT;
  ```
  session_id UNIQUE（1会議1議事録・upsert）。LATEST_SCHEMA_VERSION を 10 に更新。
- `crates/shogun-memory/src/meeting_recaps.rs`: `save(conn, session_id, &MeetingMinutes, model, now)` /
  `get(conn, session_id) -> Option<StoredRecap>`。テキストは redact 通過。in-memory DB でテスト。

### 純ロジック: プロンプト構築とパース
- `crates/shogun-core/src/meeting/minutes.rs`:
  - 型 `MeetingMinutes { summary: String, decisions: Vec<String>, next_actions: Vec<NextAction> }`,
    `NextAction { text: String, owner: Option<String> }`（serde）。
  - `build_prompt(lines: &[TranscriptLine], notes: Option<&str>, lang: &str) -> String`:
    transcript（speaker付き）＋notes を与え、**指定言語（既定en）で** JSON を返せと指示。
    決定事項/Next Action を構造で出させる。送信を伴う action も「提案」に留める旨を明記
    （不変条件4）。
  - `parse_minutes(model_output: &str) -> Result<MeetingMinutes, MinutesError>`: モデル出力から
    JSON を抽出しパース。壊れていれば Err（呼び出し側は縮退Recapのまま）。寛容にパース。
  - すべて純ロジックで単体テスト（プロンプトに transcript/notes/言語が入る、正常JSON→構造、
    壊れたJSON→Err、送信系actionが提案化）。

## Slice 2（配線・統合）
- `traceview.rs` に `Purpose::MeetingRecap` 追加。
- desktop: `Effect::BuildRecap` で当該 session の `transcript_segments`＋`session_notes` を集め、
  `minutes::build_prompt` → 既存 `AnthropicBatchClient`（Dream Cycle/Morning Brief と同じ構築経路）
  の `run(items, max_polls, sleep)` を**バックグラウンドタスク**で実行 → `parse_minutes` →
  `meeting_recaps::save` → `app.emit("meeting_recap", …)`。失敗は縮退Recapのまま（不変条件・FR-MT-19）。
- 検証: memory test + core test + desktop build。要約の実LLM実行は鍵が要るので、パースは
  固定サンプルで単体検証し、実Batchはユーザーの鍵で後日。
