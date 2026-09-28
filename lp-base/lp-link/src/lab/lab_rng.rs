//! The lab's only randomness: SplitMix64, seeded, so a run's message sizes
//! are reproducible from its seed on both ends.

#[derive(Clone, Debug)]
pub struct LabRng(u64);

impl LabRng {
    pub fn new(seed: u64) -> Self {
        LabRng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, n)`; 0 when `n` is 0.
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next_u64() % n }
    }

    /// A size in `[min, max]`, spread evenly over powers of two (log-uniform),
    /// so a 16 B – 16 KB range exercises small and large messages alike
    /// instead of mostly large ones. Integer only: the C6 has no FPU.
    pub fn size(&mut self, min: usize, max: usize) -> usize {
        let min = min.max(1);
        let max = max.max(min);
        let lo = usize::BITS - 1 - min.leading_zeros();
        let hi = usize::BITS - 1 - max.leading_zeros();
        let bucket = lo + self.below(u64::from(hi - lo + 1)) as u32;
        let start = (1usize << bucket).max(min);
        let end = (1usize << bucket)
            .saturating_mul(2)
            .saturating_sub(1)
            .min(max);
        if end <= start {
            return start;
        }
        start + self.below((end - start + 1) as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_stay_in_range_and_cover_every_octave() {
        let mut rng = LabRng::new(7);
        let mut octaves = [0u32; 16];
        for _ in 0..20_000 {
            let s = rng.size(16, 16 * 1024);
            assert!((16..=16 * 1024).contains(&s), "{s}");
            octaves[(usize::BITS - 1 - s.leading_zeros()) as usize] += 1;
        }
        for (bit, &n) in octaves.iter().enumerate().take(15).skip(4) {
            assert!(n > 1_000, "octave 2^{bit} drawn only {n} times");
        }
    }

    #[test]
    fn a_degenerate_range_is_one_size() {
        let mut rng = LabRng::new(1);
        assert_eq!(rng.size(100, 100), 100);
        assert_eq!(rng.size(100, 50), 100);
    }
}
