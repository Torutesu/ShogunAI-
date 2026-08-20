/// Reciprocal Rank Fusion over several ranked id lists (each best-first, 1-based rank). The
/// score of an id is `Σ_lists 1/(k + rank)`; `k` damps the influence of low ranks (60 is the
/// canonical default). Ids are returned sorted by descending score; ties break by smaller id
/// for determinism. Pure — no DB, fully unit-tested.
pub fn reciprocal_rank_fusion(lists: &[&[i64]], k: f64) -> Vec<(i64, f64)> {
    use std::collections::HashMap;
    let mut scores: HashMap<i64, f64> = HashMap::new();
    for list in lists {
        for (i, &id) in list.iter().enumerate() {
            let rank = (i + 1) as f64;
            *scores.entry(id).or_insert(0.0) += 1.0 / (k + rank);
        }
    }
    let mut out: Vec<(i64, f64)> = scores.into_iter().collect();
    // Descending score; deterministic tie-break by ascending id.
    out.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    out
}
