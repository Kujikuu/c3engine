//! A small seeded random number generator (SplitMix64): the simulation replays the same way
//! from the same seed, which tests and server replays rely on.

#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `min..=max` (`min` when the range is empty).
    pub fn range(&mut self, min: i64, max: i64) -> i64 {
        if max <= min {
            return min;
        }
        let span = (max - min) as u64 + 1;
        min + (self.next_u64() % span) as i64
    }

    /// True with probability `chance` out of `out_of`.
    pub fn chance(&mut self, chance: u32, out_of: u32) -> bool {
        out_of > 0 && (self.next_u64() % u64::from(out_of)) < u64::from(chance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_stay_inside_and_repeat_from_a_seed() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..1000 {
            let v = a.range(-3, 5);
            assert!((-3..=5).contains(&v));
            assert_eq!(v, b.range(-3, 5));
        }
        assert_eq!(a.range(4, 4), 4);
        assert!(!a.chance(0, 100) && a.chance(100, 100));
    }
}
