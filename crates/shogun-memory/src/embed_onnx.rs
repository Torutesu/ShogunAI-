//! The real ONNX-backed embedder (ADR-001: multilingual-e5-small, on-device, no cloud embedding
//! API). Feature-gated behind `onnx` — see the feature note in Cargo.toml for why it is off by
//! default.
//!
//! The model itself is **not** in the repository: it is a few hundred MB of weights, which git is
//! the wrong place for. It is fetched at build time and shipped inside the .app
//! (`scripts/fetch-embedding-model.sh`), and this loads it from a path at runtime. Without it the
//! product still works — search stays lexical (see `Db::with_embedder`).
//!
//! Three details are easy to get wrong and would silently degrade retrieval rather than fail:
//!
//! * **e5 role prefixes.** The model is trained with `query:` / `passage:` and its accuracy drops
//!   noticeably without them, in a way nothing surfaces as an error.
//! * **Mean pooling over the attention mask.** Padding tokens must not be averaged in, or every
//!   short text drifts toward the padding vector.
//! * **L2 normalisation.** Cosine similarity — and the vector store's distance — assume unit
//!   vectors.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Value;
use tokenizers::Tokenizer;

use crate::embed::{e5_passage, e5_query, EmbedError, Embedder, E5_SMALL_DIM};

/// Longest input handed to the model. e5-small is trained at 512; anything past that is truncated
/// by the tokenizer anyway, and a captured window can be far longer.
const MAX_TOKENS: usize = 512;

// ------------------------------------------------------------------ finding the ONNX Runtime
// `ort` is built in `load-dynamic` mode: nothing is linked at build time and the runtime library is
// resolved by `dlopen` at first use. Two problems come with that, and both bite here rather than in
// ort:
//
// 1. A bare `dlopen("libonnxruntime.dylib")` searches /usr/local/lib and /usr/lib — **not**
//    /opt/homebrew/lib, which is where `brew install onnxruntime` puts it on Apple Silicon. So the
//    documented setup steps end in a failure to load a model that is perfectly fine.
// 2. When it cannot find the library, ort **panics** instead of returning an error. A caller that
//    carefully handles a load failure — like the desktop app, which is supposed to fall back to
//    lexical search — dies anyway. That breaks the rule that a missing model degrades rather than
//    interrupts.
//
// Resolving the path before ort is touched fixes the first, and refusing up front when there is
// nothing to resolve fixes the second by keeping the failure a `Result`.

#[cfg(target_os = "macos")]
const RUNTIME_LIB: &str = "libonnxruntime.dylib";
#[cfg(not(target_os = "macos"))]
const RUNTIME_LIB: &str = "libonnxruntime.so";

/// Directories searched when `ORT_DYLIB_PATH` is unset, in order.
const RUNTIME_DIRS: &[&str] = &[
    "/opt/homebrew/lib", // Homebrew on Apple Silicon — the one a bare dlopen misses
    "/usr/local/lib",    // Homebrew on Intel, and manual installs
    "/opt/local/lib",    // MacPorts
    "/usr/lib",
];

/// Decide which runtime library to load. Pure: `exists` is injected so the decision is testable
/// without a filesystem or an installed runtime.
///
/// An explicit `ORT_DYLIB_PATH` always wins — but it is still checked, because a stale value would
/// otherwise reach ort and panic, which is exactly the failure mode this function exists to remove.
fn resolve_runtime(
    explicit: Option<PathBuf>,
    exists: impl Fn(&Path) -> bool,
) -> Result<PathBuf, String> {
    if let Some(p) = explicit {
        return if exists(&p) {
            Ok(p)
        } else {
            Err(format!("ORT_DYLIB_PATH points at a missing file: {}", p.display()))
        };
    }
    RUNTIME_DIRS
        .iter()
        .map(|d| Path::new(d).join(RUNTIME_LIB))
        .find(|p| exists(p))
        .ok_or_else(|| {
            format!(
                "{RUNTIME_LIB} not found (looked in {}) — install the ONNX Runtime \
                 (`brew install onnxruntime`) or set ORT_DYLIB_PATH",
                RUNTIME_DIRS.join(", ")
            )
        })
}

/// Point `ort` at the runtime library, or fail cleanly before it can panic.
fn ensure_runtime() -> Result<(), EmbedError> {
    let explicit = std::env::var_os("ORT_DYLIB_PATH").map(PathBuf::from);
    let already_set = explicit.is_some();
    let path = resolve_runtime(explicit, |p| p.exists()).map_err(EmbedError::Inference)?;
    if !already_set {
        // Set once, at load time, on the thread that is about to build the session — before any
        // background embedding job exists to read it.
        std::env::set_var("ORT_DYLIB_PATH", &path);
    }
    Ok(())
}

/// A loaded ONNX embedding model.
///
/// The session is behind a `Mutex` because `Embedder` takes `&self` (so it can be shared as
/// `Arc<dyn Embedder>`) while ORT's `run` needs `&mut`. Embedding happens on the background job,
/// never on the capture write path, so the lock is not on anything latency-critical.
pub struct OnnxEmbedder {
    session: Mutex<Session>,
    tokenizer: Tokenizer,
}

impl OnnxEmbedder {
    /// Load the model and its tokenizer from disk.
    ///
    /// `model` is the `.onnx` file, `tokenizer` the matching `tokenizer.json` — they must be from
    /// the same model, since a mismatched vocabulary produces confident nonsense rather than an
    /// error.
    pub fn load(model: impl AsRef<Path>, tokenizer: impl AsRef<Path>) -> Result<Self, EmbedError> {
        ensure_runtime()?;
        let tokenizer = Tokenizer::from_file(tokenizer.as_ref())
            .map_err(|e| EmbedError::Inference(format!("tokenizer: {e}")))?;
        let model = model.as_ref().to_path_buf();
        // Belt and braces around the one thing that can still abort the process: ort panics on
        // several load failures (a wrong-architecture library, a corrupt graph) rather than
        // returning them. Nothing here is reused after a panic — the error is returned and the
        // caller stays on lexical search — so unwinding out of ort is safe to absorb.
        let session = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Session::builder()
                .and_then(|b| b.with_optimization_level(GraphOptimizationLevel::Level3))
                // One thread: this runs on a background job and must not fight the capture daemon
                // or the UI for cores (the idle-CPU SLO is 5%).
                .and_then(|b| b.with_intra_threads(1))
                .and_then(|b| b.commit_from_file(&model))
        }))
        .map_err(|_| EmbedError::Inference("onnx runtime aborted while loading the model".into()))?
        .map_err(|e| EmbedError::Inference(format!("session: {e}")))?;
        Ok(Self { session: Mutex::new(session), tokenizer })
    }

    /// Embed a batch of already-prefixed texts.
    fn embed_prepared(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let mut encodings = self
            .tokenizer
            .encode_batch(texts.iter().map(String::as_str).collect::<Vec<_>>(), true)
            .map_err(|e| EmbedError::Inference(format!("encode: {e}")))?;
        for e in &mut encodings {
            e.truncate(MAX_TOKENS, 0, tokenizers::TruncationDirection::Right);
        }
        let batch = encodings.len();
        let seq = encodings.iter().map(|e| e.get_ids().len()).max().unwrap_or(0);
        if seq == 0 {
            return Err(EmbedError::Inference("empty tokenization".into()));
        }

        // Pad to a rectangular batch, keeping the mask so padding can be excluded from the mean.
        let mut ids = vec![0i64; batch * seq];
        let mut mask = vec![0i64; batch * seq];
        let mut types = vec![0i64; batch * seq];
        for (r, enc) in encodings.iter().enumerate() {
            for (c, (&id, &m)) in
                enc.get_ids().iter().zip(enc.get_attention_mask().iter()).enumerate()
            {
                ids[r * seq + c] = id as i64;
                mask[r * seq + c] = m as i64;
            }
            let _ = &mut types;
        }

        let shape = [batch, seq];
        let id_t = Value::from_array((shape, ids)).map_err(inference)?;
        let mask_t = Value::from_array((shape, mask.clone())).map_err(inference)?;
        let type_t = Value::from_array((shape, types)).map_err(inference)?;

        let mut session = self
            .session
            .lock()
            .map_err(|_| EmbedError::Inference("embedder lock poisoned".into()))?;
        // Some e5 exports take token_type_ids and some don't. Ask the model which it wants rather
        // than running it and catching the failure: a failed run is indistinguishable from a real
        // inference error, and would turn a shape mismatch into a silent fallback.
        let wants_types = session.inputs.iter().any(|i| i.name == "token_type_ids");
        let outputs = if wants_types {
            session.run(ort::inputs![
                "input_ids" => id_t,
                "attention_mask" => mask_t,
                "token_type_ids" => type_t,
            ])
        } else {
            session.run(ort::inputs![
                "input_ids" => id_t,
                "attention_mask" => mask_t,
            ])
        }
        .map_err(inference)?;

        let (out_shape, data) = outputs[0].try_extract_tensor::<f32>().map_err(inference)?;
        // last_hidden_state: [batch, seq, hidden]
        if out_shape.len() != 3 {
            return Err(EmbedError::Inference(format!("unexpected output rank {}", out_shape.len())));
        }
        let hidden = out_shape[2] as usize;
        if hidden != E5_SMALL_DIM {
            return Err(EmbedError::Dim { expected: E5_SMALL_DIM, got: hidden });
        }

        let mut out = Vec::with_capacity(batch);
        for r in 0..batch {
            let mut sum = vec![0f32; hidden];
            let mut kept = 0f32;
            for c in 0..seq {
                if mask[r * seq + c] == 0 {
                    continue; // padding must not pull the mean toward the pad vector
                }
                kept += 1.0;
                let base = (r * seq + c) * hidden;
                for (h, s) in sum.iter_mut().enumerate() {
                    *s += data[base + h];
                }
            }
            if kept > 0.0 {
                for s in sum.iter_mut() {
                    *s /= kept;
                }
            }
            l2_normalize(&mut sum);
            out.push(sum);
        }
        Ok(out)
    }
}

fn inference<E: std::fmt::Display>(e: E) -> EmbedError {
    EmbedError::Inference(e.to_string())
}

/// Scale to unit length. Cosine similarity and the vector store both assume it; a zero vector is
/// left alone rather than dividing by zero.
fn l2_normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > f32::EPSILON {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

impl Embedder for OnnxEmbedder {
    fn dim(&self) -> usize {
        E5_SMALL_DIM
    }

    fn embed_passages(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
        let prepared: Vec<String> = texts.iter().map(|t| e5_passage(t)).collect();
        self.embed_prepared(&prepared)
    }

    fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
        let prepared = vec![e5_query(text)];
        self.embed_prepared(&prepared)?
            .into_iter()
            .next()
            .ok_or_else(|| EmbedError::Inference("no vector returned".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalisation_makes_a_unit_vector() {
        let mut v = vec![3.0, 4.0];
        l2_normalize(&mut v);
        assert!((v[0] - 0.6).abs() < 1e-6 && (v[1] - 0.8).abs() < 1e-6);
        let len = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((len - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_zero_vector_is_left_alone_rather_than_dividing_by_zero() {
        let mut v = vec![0.0, 0.0, 0.0];
        l2_normalize(&mut v);
        assert!(v.iter().all(|x| *x == 0.0), "must not produce NaN");
    }

    /// Loading is what fails on a real machine when the model is missing or mismatched, so it must
    /// report rather than panic.
    #[test]
    fn a_missing_model_is_an_error_not_a_panic() {
        let err = OnnxEmbedder::load("/nonexistent/model.onnx", "/nonexistent/tokenizer.json");
        assert!(err.is_err());
    }

    /// Homebrew on Apple Silicon installs to /opt/homebrew/lib, which a bare `dlopen` never
    /// searches. Missing it is what turns "follow the setup doc" into "model present but failed to
    /// load", so the search order is worth pinning.
    #[test]
    fn the_runtime_is_found_where_homebrew_actually_puts_it() {
        let at = |dir: &'static str| {
            let want = Path::new(dir).join(RUNTIME_LIB);
            resolve_runtime(None, move |p| p == want)
        };
        for dir in RUNTIME_DIRS {
            assert_eq!(at(dir).as_deref(), Ok(Path::new(dir).join(RUNTIME_LIB).as_path()), "{dir}");
        }
        assert!(
            RUNTIME_DIRS.contains(&"/opt/homebrew/lib"),
            "the Apple Silicon Homebrew prefix is the whole reason this search exists"
        );
    }

    /// Nothing installed must be a `Result`, not a panic from inside ort — the app is supposed to
    /// carry on with lexical search.
    #[test]
    fn no_runtime_anywhere_is_a_reported_error() {
        let err = resolve_runtime(None, |_| false).unwrap_err();
        assert!(err.contains(RUNTIME_LIB) && err.contains("ORT_DYLIB_PATH"), "{err}");
    }

    /// An explicit path wins, but a stale one is caught here rather than reaching ort.
    #[test]
    fn an_explicit_path_wins_but_is_still_checked() {
        let chosen = PathBuf::from("/somewhere/else/libonnxruntime.dylib");
        assert_eq!(resolve_runtime(Some(chosen.clone()), |_| true), Ok(chosen.clone()));
        assert!(resolve_runtime(Some(chosen), |_| false).is_err());
    }

    /// End-to-end against the real model. Ignored: CI has no model, and this must never depend on
    /// one being present.
    ///   SHOGUN_EMBED_MODEL=…/model.onnx SHOGUN_EMBED_TOKENIZER=…/tokenizer.json \
    ///     cargo test -p shogun-memory --features onnx -- --ignored --nocapture
    #[test]
    #[ignore = "needs the bundled model; set SHOGUN_EMBED_MODEL / SHOGUN_EMBED_TOKENIZER"]
    fn the_real_model_places_related_text_closer_than_unrelated() {
        let (Ok(model), Ok(tok)) =
            (std::env::var("SHOGUN_EMBED_MODEL"), std::env::var("SHOGUN_EMBED_TOKENIZER"))
        else {
            return;
        };
        let e = OnnxEmbedder::load(model, tok).expect("load");
        assert_eq!(e.dim(), E5_SMALL_DIM);

        let q = e.embed_query("what did we decide about the vendor pricing?").unwrap();
        let passages = e
            .embed_passages(&[
                "The vendor renewal was settled at 12k for the year.",
                "Lunch options near the office on Thursday.",
            ])
            .unwrap();
        let related = crate::embed::cosine_similarity(&q, &passages[0]);
        let unrelated = crate::embed::cosine_similarity(&q, &passages[1]);
        eprintln!("related={related:.4} unrelated={unrelated:.4}");
        assert!(
            related > unrelated,
            "the model must rank the answering passage higher: {related} vs {unrelated}"
        );
        // Unit length, as the vector store assumes.
        let len = q.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((len - 1.0).abs() < 1e-3, "query vector must be normalised: {len}");
    }
}
