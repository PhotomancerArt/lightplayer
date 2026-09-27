//! A fresh lp-link nonce per port open, on either side of the wasm line.
//!
//! A link's nonce is what tells the board this is a new host session (a
//! reopened port, a reloaded page): it must differ between opens, and it
//! does not have to be secret.

/// A nonce for a new host link (never 0).
pub fn fresh_link_nonce() -> u32 {
    #[cfg(all(target_arch = "wasm32", feature = "emulator-tab"))]
    {
        ((js_sys::Math::random() * f64::from(u32::MAX)) as u32) | 1
    }
    #[cfg(not(all(target_arch = "wasm32", feature = "emulator-tab")))]
    {
        native_nonce()
    }
}

#[cfg(not(all(target_arch = "wasm32", feature = "emulator-tab")))]
fn native_nonce() -> u32 {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static OPENS: AtomicU32 = AtomicU32::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or(0);
    let mut z = nanos
        ^ (u64::from(OPENS.fetch_add(1, Ordering::Relaxed)) << 40)
        ^ (u64::from(std::process::id()) << 20);
    // SplitMix64's finalizer: spreads every input bit over the output.
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    ((z ^ (z >> 31)) as u32) | 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_opens_get_two_nonces() {
        assert_ne!(fresh_link_nonce(), fresh_link_nonce());
    }
}
