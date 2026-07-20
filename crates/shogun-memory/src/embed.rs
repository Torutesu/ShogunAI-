//! Local embeddings (FR-MEM-21, ADR-001) — the model-agnostic core.
//!
//! v1 bundles the multilingual **e5-small** ONNX model (384-dim, JA+EN, offline, no cloud
//! embedding API). This module defines the [`Embedder`] abstraction the search layer depends
//! on, the e5 input-format helpers (e5 models require `query:` / `passage:` prefixes), a pure
//! cosine similarity, and a deterministic [`MockEmbedder`] so the storage + Warm-layer vector
//! search can be tested without shipping the model in CI.
//!
//! The real ONNX-backed embedder (ort + tokenizers, loaded at runtime from the bundled model)
//! implements the same trait; the async embed job (FR-MEM-22, non-blocking write path) and the
//! sqlite-vec Warm store build on top of this.

/// e5-small output dimensionality (ADR-001 / the chosen model).
pub const E5_SMALL_DIM: usize = 384;

/// Errors from embedding.
#[derive(Debug, thiserror::Error)]
pub enum EmbedError {
    #[error("model not loaded")]
    NotLoaded,
    #[error("inference failed: {0}")]
    Inference(String),
    #[error("dimension mismatch: expected {expected}, got {got}")]
    Dim { expected: usize, got: usize },
}

/// Produces embedding vectors. Implemented by the bundled ONNX model in production and by
/// [`MockEmbedder`] in tests. `Send + Sync` so the async embed job can hold one across tasks.
pub trait Embedder: Send + Sync {
    fn dim(&self) -> usize;
    /// Embed documents to store (e5 `passage:` role). One vector per input, each of length
    /// [`Embedder::dim`].
    fn embed_passages(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError>;
    /// Embed a search query (e5 `query:` role).
    fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbedError>;
}

/// e5 input formatting: the model is trained with these role prefixes and degrades without
/// them. The ONNX embedder applies these before tokenizing; exposed so the contract is explicit.
pub fn e5_query(text: &str) -> String {
    format!("query: {text}")
}

pub fn e5_passage(text: &str) -> String {
    format!("passage: {text}")
}

/// Cosine similarity of two equal-length vectors, in [-1, 1]. Returns 0.0 for a zero vector or
/// a length mismatch (both are "no signal" rather than an error on the hot search path).
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// L2-normalize a vector in place (unit length). A zero vector is left unchanged.
pub fn l2_normalize(v: &mut [f32]) {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

/// A deterministic, model-free embedder for tests: hashes token bytes into a fixed-dim vector
/// and L2-normalizes it. Similar strings get similar vectors (shared tokens), so it exercises
/// the store + cosine-ranking path meaningfully without the real model. NOT for production.
#[derive(Debug, Clone)]
pub struct MockEmbedder {
    dim: usize,
}

impl MockEmbedder {
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }

    fn embed_one(&self, text: &str) -> Vec<f32> {
        let mut v = vec![0.0f32; self.dim];
        // Bag-of-words hashing: each whitespace token bumps one dimension. Case-insensitive.
        for tok in text.to_lowercase().split_whitespace() {
            let mut h: u64 = 1469598103934665603; // FNV-1a offset basis
            for b in tok.bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(1099511628211);
            }
            let idx = (h % self.dim as u64) as usize;
            v[idx] += 1.0;
        }
        l2_normalize(&mut v);
        v
    }
}

impl Embedder for MockEmbedder {
    fn dim(&self) -> usize {
        self.dim
    }

    fn embed_passages(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
        Ok(texts.iter().map(|t| self.embed_one(&e5_passage(t))).collect())
    }

    fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
        Ok(self.embed_one(&e5_query(text)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_of_identical_is_one() {
        let a = vec![1.0, 2.0, 3.0];
        assert!((cosine_similarity(&a, &a) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_of_orthogonal_is_zero() {
        assert_eq!(cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]), 0.0);
    }

    #[test]
    fn cosine_handles_zero_and_mismatch() {
        assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
        assert_eq!(cosine_similarity(&[1.0], &[1.0, 2.0]), 0.0);
    }

    #[test]
    fn l2_normalize_gives_unit_length() {
        let mut v = vec![3.0, 4.0];
        l2_normalize(&mut v);
        let len: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((len - 1.0).abs() < 1e-6);
    }

    #[test]
    fn e5_prefixes_are_applied() {
        assert_eq!(e5_query("hi"), "query: hi");
        assert_eq!(e5_passage("hi"), "passage: hi");
    }

    #[test]
    fn mock_embedder_dim_and_normalization() {
        let m = MockEmbedder::new(E5_SMALL_DIM);
        let v = m.embed_query("hello world").unwrap();
        assert_eq!(v.len(), E5_SMALL_DIM);
        let len: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((len - 1.0).abs() < 1e-5);
    }

    #[test]
    fn mock_similar_texts_rank_above_dissimilar() {
        let m = MockEmbedder::new(E5_SMALL_DIM);
        let q = m.embed_query("quarterly budget review").unwrap();
        let close = m.embed_passages(&["the quarterly budget review meeting"]).unwrap()[0].clone();
        let far = m.embed_passages(&["lunch plans for saturday"]).unwrap()[0].clone();
        assert!(
            cosine_similarity(&q, &close) > cosine_similarity(&q, &far),
            "shared tokens should rank closer"
        );
    }
}
