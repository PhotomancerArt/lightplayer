//! A fresh lp-link nonce per port open.
//!
//! A link's nonce is what tells the other end this is a new session (a new
//! host, a reopened port): it must differ between opens, and it does not
//! have to be secret. The wall clock's nanoseconds, the process id and a
//! per-process counter, mixed, are enough for that; no RNG dependency.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A nonce for a new host link (never 0).
pub fn fresh_link_nonce() -> u32 {
    static OPENS: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or(0);
    let seed = nanos
        ^ (u64::from(OPENS.fetch_add(1, Ordering::Relaxed)) << 40)
        ^ (u64::from(std::process::id()) << 20);
    (split_mix(seed) as u32) | 1
}

/// SplitMix64's finalizer: spreads every input bit over the output.
fn split_mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_opens_get_two_nonces() {
        let a = fresh_link_nonce();
        let b = fresh_link_nonce();
        assert_ne!(a, b);
        assert_ne!(a, 0);
    }
}
