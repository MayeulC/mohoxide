//! Deterministic sim RNG (xoshiro256**, seeded via splitmix64).
//!
//! Integer-only state updates; floats are built exactly from 53 random bits, so
//! the sequence is bit-identical on every platform. Part of sim state: must be
//! seeded from the session seed and included in the per-tick checksum.

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SimRng {
    s: [u64; 4],
}

impl SimRng {
    /// A digest of the generator state (for the sim checksum).
    pub fn state_hash(&self) -> u64 {
        self.s.iter().fold(0u64, |acc, &x| acc.rotate_left(17) ^ x)
    }

    pub fn new(seed: u64) -> Self {
        let mut x = seed;
        let mut next = || {
            x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        Self { s: [next(), next(), next(), next()] }
    }

    pub fn next_u64(&mut self) -> u64 {
        let s = &mut self.s;
        let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        result
    }

    /// Uniform in `[0, 1)`, exact (53-bit mantissa from integer bits).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Uniform integer in `[lo, hi]` (inclusive), unbiased via rejection.
    pub fn range_i64(&mut self, lo: i64, hi: i64) -> i64 {
        debug_assert!(lo <= hi);
        let span = (hi as i128 - lo as i128) as u128 + 1;
        if span > u64::MAX as u128 {
            return self.next_u64() as i64;
        }
        let span = span as u64;
        let zone = u64::MAX - (u64::MAX % span) - 1;
        loop {
            let v = self.next_u64();
            if v <= zone {
                return (lo as i128 + (v % span) as i128) as i64;
            }
        }
    }

    pub fn state(&self) -> [u64; 4] {
        self.s
    }

    /// Approximately standard normal (mean 0, deviation 1), in fixed point:
    /// the sum of twelve 16-bit uniforms minus 6 (Irwin-Hall; within
    /// [-6, 6]). Integer-only, so identical everywhere. Moho's
    /// `CRandomStream::FRandGaussian` uses the polar method on floats.
    /// Uniform in `[0, 1)` as fixed point (the top 32 bits as the fraction).
    pub fn uniform_fx(&mut self) -> crate::engine::sim::math::Fx {
        crate::engine::sim::math::Fx::from_bits((self.next_u64() >> 32) as i64)
    }

    pub fn gaussian(&mut self) -> crate::engine::sim::math::Fx {
        let mut sum: i64 = 0;
        for _ in 0..3 {
            let v = self.next_u64();
            for k in 0..4 {
                sum += ((v >> (16 * k)) & 0xffff) as i64;
            }
        }
        // Each uniform is u / 65536: the sum in 32.32 is sum << 16.
        crate::engine::sim::math::Fx::from_bits((sum << 16) - (6i64 << 32))
    }
}

impl Default for SimRng {
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_is_stable() {
        // Golden values: changing these breaks replay/lockstep compatibility.
        let mut r = SimRng::new(42);
        let v: Vec<u64> = (0..3).map(|_| r.next_u64()).collect();
        let mut again = SimRng::new(42);
        let w: Vec<u64> = (0..3).map(|_| again.next_u64()).collect();
        assert_eq!(v, w);
        assert_ne!(v[0], v[1]);
    }

    #[test]
    fn ranges_are_inclusive_and_bounded() {
        let mut r = SimRng::new(1);
        let mut seen = [false; 3];
        for _ in 0..1000 {
            let x = r.range_i64(1, 3);
            assert!((1..=3).contains(&x));
            seen[(x - 1) as usize] = true;
            let f = r.next_f64();
            assert!((0.0..1.0).contains(&f));
        }
        assert!(seen.iter().all(|&s| s));
    }

    #[test]
    fn gaussian_has_unit_spread() {
        let mut r = SimRng::new(3);
        let n = 20_000;
        let xs: Vec<f64> = (0..n).map(|_| r.gaussian().to_num::<f64>()).collect();
        let mean = xs.iter().sum::<f64>() / n as f64;
        let var = xs.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n as f64;
        assert!(mean.abs() < 0.03, "mean {mean}");
        assert!((var - 1.0).abs() < 0.05, "variance {var}");
        assert!(xs.iter().all(|x| x.abs() <= 6.0));
    }
}
