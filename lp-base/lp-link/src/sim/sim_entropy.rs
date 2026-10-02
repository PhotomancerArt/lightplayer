//! Entropy for secure links under the simulator and in tests: a seeded,
//! per-thread SplitMix64 behind the plain `fn(&mut [u8])` that
//! `Link::new_secure` takes, so a run stays reproducible from its seed.
//! Never a source for a real link: an edge injects its platform's RNG.

use core::cell::Cell;

use crate::sim::sim_rng::SimRng;

std::thread_local! {
    static STATE: Cell<u64> = const { Cell::new(0x5EC0_4E11_2026_1001) };
}

/// Restart this thread's stream at `seed`.
pub fn seed(seed: u64) {
    STATE.with(|s| s.set(seed));
}

/// Fill `buf` from this thread's stream (the `entropy` a simulated secure
/// link is built with).
pub fn fill(buf: &mut [u8]) {
    STATE.with(|s| {
        let mut rng = SimRng::new(s.get());
        for chunk in buf.chunks_mut(8) {
            let v = rng.next_u64().to_le_bytes();
            chunk.copy_from_slice(&v[..chunk.len()]);
        }
        s.set(rng.next_u64());
    });
}
