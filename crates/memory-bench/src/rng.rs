//! Deterministic PRNG for workload generation.
//!
//! SplitMix64 (Steele et al., 2014), written out here rather than pulled from `rand`. A
//! benchmark's whole value is that a seed reproduces a workload — including six months from now,
//! against a different crate lockfile, on a different OS. `rand`'s generators are explicitly
//! allowed to change their output between minor versions, which would silently invalidate every
//! stored baseline. This one cannot change, because it is fourteen lines and lives in the repo.

/// A seeded, reproducible source of pseudorandom numbers.
///
/// Identical `seed` values yield identical sequences on every platform: the arithmetic is all
/// wrapping 64-bit integer ops with fixed constants, so there is no float, no address, and no
/// hash-map iteration order anywhere in the path.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Next raw 64 bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, n)`. Returns 0 when `n == 0` rather than dividing by zero.
    ///
    /// Uses Lemire's multiply-shift instead of `%`: modulo biases the low values whenever `n`
    /// does not divide 2^64, which for a duplicate-rate workload would quietly skew exactly the
    /// distribution being measured.
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            return 0;
        }
        ((self.next_u64() as u128 * n as u128) >> 64) as u64
    }

    /// Uniform in `[0, n)` as a `usize`.
    pub fn index(&mut self, n: usize) -> usize {
        self.below(n as u64) as usize
    }

    /// `true` with probability `p` (clamped to `[0, 1]`).
    pub fn chance(&mut self, p: f64) -> bool {
        let p = p.clamp(0.0, 1.0);
        // 2^53 keeps the ratio exactly representable as f64.
        let scale = 1u64 << 53;
        (self.below(scale) as f64) < p * scale as f64
    }

    /// Pick one element. Returns `None` for an empty slice.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            return None;
        }
        items.get(self.index(items.len()))
    }

    /// Fisher-Yates, so a shuffled order is reproducible from the seed too.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.index(i + 1);
            items.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        let xs: Vec<u64> = (0..64).map(|_| a.next_u64()).collect();
        let ys: Vec<u64> = (0..64).map(|_| b.next_u64()).collect();
        assert_eq!(xs, ys);
    }

    #[test]
    fn different_seed_different_sequence() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(43);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    /// Guards the stored baselines: if this changes, every recorded workload silently changed
    /// with it, and old reports stop being comparable to new ones.
    #[test]
    fn sequence_is_pinned_to_known_values() {
        let mut r = Rng::new(42);
        assert_eq!(r.next_u64(), 13679457532755275413);
        assert_eq!(r.next_u64(), 2949826092126892291);
        assert_eq!(r.next_u64(), 5139283748462763858);
    }

    #[test]
    fn below_stays_in_range_and_handles_zero() {
        let mut r = Rng::new(7);
        for _ in 0..1000 {
            assert!(r.below(10) < 10);
        }
        assert_eq!(r.below(0), 0);
    }

    #[test]
    fn chance_bounds_are_absolute() {
        let mut r = Rng::new(9);
        for _ in 0..200 {
            assert!(r.chance(1.0));
            assert!(!r.chance(0.0));
        }
    }

    #[test]
    fn shuffle_is_a_permutation_and_reproducible() {
        let mut a: Vec<u32> = (0..50).collect();
        let mut b = a.clone();
        Rng::new(5).shuffle(&mut a);
        Rng::new(5).shuffle(&mut b);
        assert_eq!(a, b);
        let mut sorted = a.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..50).collect::<Vec<u32>>());
    }
}
