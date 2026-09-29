//! The classic's link nonce: the chip's random word, salted by a boot count
//! that survives a software reset (ruling DD28 of the classic-UART plan).
//!
//! A link's nonce is how a host that stayed attached learns the board
//! restarted: a SYN with a new nonce resets the host's session
//! (`PeerRestarted`), which fails the requests it lost at once. A repeated
//! nonce hides that restart. The C6 draws its nonce from its RNG, which a
//! radio keeps fed; the classic runs no radio, and whether its `WDEV_RND_REG`
//! gives a different word after a software reset (a Reboot request) is not
//! something anyone has measured. So the chip crate keeps a counter in RTC
//! fast RAM, which a software reset does not clear, bumps it every boot, and
//! salts the random word with it here.
//!
//! The salt is added through an odd multiplier, a bijection modulo 2^32, so
//! two boots whose random words are **equal** get nonces that differ whenever
//! their counters differ, which consecutive boots' always do. When the random
//! words differ the result is as random as they are. An EN-pin or power-on
//! reset clears RTC fast RAM (the counter starts from whatever it holds at
//! power-up); on those resets the ESP-IDF bootloader's entropy stir is what
//! moves the random word.

/// The salt's multiplier: odd (so the map `salt -> salt * K` is a bijection
/// modulo 2^32) and far from small, so consecutive salts land far apart.
const SALT_MULTIPLIER: u32 = 0x9E37_79B1;

/// The session nonce for this boot, from the chip's random word and this
/// boot's count of boots (see the module docs).
pub fn session_nonce(random: u32, boot_count: u32) -> u32 {
    random.wrapping_add(boot_count.wrapping_mul(SALT_MULTIPLIER))
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    /// The case the salt exists for: the RNG gives the same word after a
    /// software reset. Consecutive boots still differ, for any word and any
    /// count, including across the counter's wrap.
    #[test]
    fn a_repeated_random_word_still_gives_consecutive_boots_new_nonces() {
        for random in [0, 1, 0x1234_5678, u32::MAX] {
            for count in [0, 1, 2, 1_000, u32::MAX - 1, u32::MAX] {
                let this = session_nonce(random, count);
                let next = session_nonce(random, count.wrapping_add(1));
                assert_ne!(this, next, "word {random:#x}, boots {count} and {count}+1");
            }
        }
    }

    /// Nor does a same-word board repeat any of its last several nonces: a
    /// host that saw a board through a few quick reboots never sees an old
    /// nonce come back.
    #[test]
    fn a_run_of_reboots_on_one_word_never_repeats_a_nonce() {
        let mut seen = std::collections::BTreeSet::new();
        for count in 0..4_096u32 {
            assert!(seen.insert(session_nonce(0xB0A2_0001, count)), "boot {count}");
        }
    }

    /// For a fixed count the salt is a shift of the random word: a word the
    /// RNG chose at random stays a nonce chosen at random.
    #[test]
    fn a_fixed_count_keeps_distinct_words_distinct() {
        let count = 7;
        let words = [0u32, 1, 2, 0xFFFF_FFFF, 0x8000_0000];
        let nonces: std::collections::BTreeSet<u32> =
            words.iter().map(|&w| session_nonce(w, count)).collect();
        assert_eq!(nonces.len(), words.len());
    }
}
