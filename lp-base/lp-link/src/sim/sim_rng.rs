//! The simulator's only source of randomness: SplitMix64, seeded, so every
//! run is reproducible from its seed.

pub struct SimRng(u64);

impl SimRng {
    pub fn new(seed: u64) -> Self {
        SimRng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Uniform in `[0, 1)`.
    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn chance(&mut self, p: f64) -> bool {
        p > 0.0 && self.f64() < p
    }

    /// Uniform in `[0, n)`; 0 when `n` is 0.
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next_u64() % n }
    }

    /// An independent stream.
    pub fn fork(&mut self) -> SimRng {
        SimRng(self.next_u64())
    }
}
