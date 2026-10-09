//! Small deterministic pseudo random number generator (SplitMix64).
//! Seedable so that tests are reproducible.

#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng { state: seed }
    }

    /// Seeds from the system clock.
    pub fn from_time() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x5eed);
        Rng::new(nanos ^ (std::process::id() as u64).rotate_left(32))
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform value in `0..bound`. `bound` must be non-zero.
    pub fn below(&mut self, bound: usize) -> usize {
        assert!(bound > 0, "bound must be non-zero");
        ((self.next_u64() as u128 * bound as u128) >> 64) as usize
    }

    /// Uniform value in `0.0..1.0`.
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            Some(&items[self.below(items.len())])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRAWS: usize = 1000;

    #[test]
    fn same_seed_gives_same_sequence() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_give_different_sequences() {
        assert_ne!(Rng::new(1).next_u64(), Rng::new(2).next_u64());
    }

    #[test]
    fn below_stays_in_range() {
        let mut rng = Rng::new(7);
        assert!((0..DRAWS).all(|_| rng.below(6) < 6));
    }

    #[test]
    fn below_covers_the_whole_range() {
        let mut rng = Rng::new(7);
        let mut seen = [false; 6];
        for _ in 0..DRAWS {
            seen[rng.below(6)] = true;
        }
        assert_eq!(seen, [true; 6]);
    }

    #[test]
    fn next_f32_is_in_unit_interval() {
        let mut rng = Rng::new(9);
        assert!((0..DRAWS).all(|_| (0.0..1.0).contains(&rng.next_f32())));
    }

    #[test]
    fn pick_from_empty_slice_is_none() {
        let empty: [u8; 0] = [];
        assert_eq!(Rng::new(3).pick(&empty), None);
    }

    #[test]
    fn pick_from_a_single_item_returns_it() {
        assert_eq!(Rng::new(3).pick(&[5]), Some(&5));
    }
}
