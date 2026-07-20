//! int8 quantization for the Cold layer (FR-MEM-04, ADR-001).
//!
//! The Cold tier holds the full history at int8 precision to keep long-horizon memory affordable
//! on-device (a 384-dim f32 vector is 1536 bytes; int8 is 384 bytes + a 4-byte scale — ~4× smaller).
//! Warm-layer embeddings are L2-normalized, so every component sits in `[-1, 1]`; a symmetric
//! per-vector scale (`max|component| / 127`) therefore uses the full int8 range with minimal error.
//!
//! Cosine similarity is scale-invariant, so quantization barely perturbs ranking — the round-trip
//! test asserts cosine ≥ 0.999 against the original. Cold is an archive, not a normal search target
//! (FR-MEM-03: routine vector search stays on Warm), so this precision loss is acceptable by design.

/// int8 range endpoint used for the symmetric scale.
const I8_MAX: f32 = 127.0;

/// Symmetric per-vector int8 quantization. Returns the int8 codes and the scale `s` such that
/// `f ≈ (code as f32) * s`. An all-zero (or denormal) vector yields a zero scale and zero codes.
pub fn quantize_i8(v: &[f32]) -> (Vec<i8>, f32) {
    let max_abs = v.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
    if max_abs == 0.0 || !max_abs.is_finite() {
        return (vec![0i8; v.len()], 0.0);
    }
    let scale = max_abs / I8_MAX;
    let codes = v
        .iter()
        .map(|&x| {
            // round-half-away, then clamp into i8 (guards the max_abs component landing on ±127).
            let q = (x / scale).round().clamp(-I8_MAX, I8_MAX);
            q as i8
        })
        .collect();
    (codes, scale)
}

/// Reconstruct an f32 vector from its int8 codes and scale.
pub fn dequantize_i8(codes: &[i8], scale: f32) -> Vec<f32> {
    codes.iter().map(|&c| c as f32 * scale).collect()
}

/// Pack int8 codes into a byte blob (two's-complement, one byte per code) for BLOB storage.
pub fn pack_i8(codes: &[i8]) -> Vec<u8> {
    codes.iter().map(|&c| c as u8).collect()
}

/// Unpack a byte blob back into int8 codes.
pub fn unpack_i8(bytes: &[u8]) -> Vec<i8> {
    bytes.iter().map(|&b| b as i8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::{cosine_similarity, l2_normalize};

    fn normalized(seed: u64, dim: usize) -> Vec<f32> {
        // deterministic, well-distributed pseudo-vector, then L2-normalized like a real embedding.
        // Mixing the index through a large odd multiplier keeps components varied (avoids the f32
        // precision collapse a plain `huge_u64 as f32 % k` would cause).
        let mut v: Vec<f32> = (0..dim)
            .map(|i| {
                let h = (i as u64)
                    .wrapping_mul(2_654_435_761)
                    .wrapping_add(seed.wrapping_mul(40_503));
                ((h % 1009) as f32) / 1009.0 - 0.5
            })
            .collect();
        l2_normalize(&mut v);
        v
    }

    #[test]
    fn round_trip_preserves_cosine() {
        let v = normalized(42, 384);
        let (codes, scale) = quantize_i8(&v);
        assert_eq!(codes.len(), v.len());
        let back = dequantize_i8(&codes, scale);
        let cos = cosine_similarity(&v, &back);
        assert!(cos >= 0.999, "int8 round-trip cosine too low: {cos}");
    }

    #[test]
    fn pack_unpack_is_identity_including_negatives() {
        let codes: Vec<i8> = vec![-128, -1, 0, 1, 127, -64, 63];
        let bytes = pack_i8(&codes);
        assert_eq!(bytes.len(), codes.len());
        assert_eq!(unpack_i8(&bytes), codes);
    }

    #[test]
    fn all_zero_vector_quantizes_to_zero_scale() {
        let (codes, scale) = quantize_i8(&[0.0; 8]);
        assert_eq!(scale, 0.0);
        assert!(codes.iter().all(|&c| c == 0));
        // dequantizing a zero-scale vector is all zeros (no NaN)
        assert!(dequantize_i8(&codes, scale).iter().all(|&x| x == 0.0));
    }

    #[test]
    fn max_component_lands_on_the_endpoint() {
        // the largest-magnitude component should map to ±127 exactly, using the full range.
        let v = vec![0.5, -1.0, 0.25];
        let (codes, _) = quantize_i8(&v);
        assert_eq!(codes[1], -127);
    }

    #[test]
    fn two_distinct_vectors_stay_distinguishable() {
        let a = normalized(1, 384);
        let b = normalized(999, 384);
        let (qa, sa) = quantize_i8(&a);
        let (qb, sb) = quantize_i8(&b);
        let da = dequantize_i8(&qa, sa);
        let db = dequantize_i8(&qb, sb);
        // self-similarity after quant stays higher than cross-similarity (ranking survives)
        assert!(cosine_similarity(&a, &da) > cosine_similarity(&a, &db));
    }
}
