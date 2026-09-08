//! `rodata_stride` — the *load* side of the cache, one kernel per stride.
//!
//! A walk through a large immutable `static` at a fixed byte stride,
//! sum-reducing so nothing can be optimised away. Every stride does the same
//! number of accesses ([`ACCESSES`]), wrapping through the array as many
//! times as it takes, so the kernels differ in **locality and nothing else** —
//! same access count, same arithmetic, same loop.
//!
//! ## Sizes, and the hypothesis they are sized against
//!
//! The array is [`RODATA_BYTES`] = 256 KiB, sized to exceed a **32 KiB** L1
//! cache by 8×. As with `code_walk`, the 32 KiB is a hypothesis and not a
//! citation: the C6's cache geometry is not in this repository (`notes.md`
//! F7, esp-hal and esp-metadata carry no cache constant for the part), and
//! **establishing it is P3's job (OQ3)** from the TRM's Cache chapter and the
//! ROM's `Cache_*` writes into EXTMEM. Nothing here hardcodes a line length,
//! a way count or a cache size as fact.
//!
//! The strides are [`STRIDES`]: 16, 32, 64, 256, 1024 and 4096 bytes. The
//! first three bracket every plausible line length; the last three walk out
//! past it. **The flash MMU's 64 KiB page stride is not among them**, and
//! deliberately so: a 256 KiB array holds four such pages, and four accesses
//! per pass repeated to 65,536 accesses would measure a cache that is warm
//! after the first pass rather than a page walk. Measuring the page stride
//! needs an array this probe does not carry; the calibration report says so
//! rather than reporting a stride that means something else.

/// The array's size in bytes: 8× a 32 KiB L1 hypothesis (see the module docs).
pub const RODATA_BYTES: usize = 256 * 1024;

const WORDS: usize = RODATA_BYTES / 4;

/// Accesses per kernel, the same for every stride, so that the strides differ
/// only in locality.
pub const ACCESSES: u32 = 65_536;

/// The byte strides walked, one kernel each.
pub const STRIDES: [usize; 6] = [16, 32, 64, 256, 1024, 4096];

/// A `static` (not a `static mut`, not a `const`) so it is one object in
/// `.rodata` rather than a value inlined at each use. Filled with a
/// non-constant pattern so that no part of it can be folded away and so the
/// sum is a real reduction over real loads.
static TABLE: [u32; WORDS] = build_table();

const fn build_table() -> [u32; WORDS] {
    let mut table = [0u32; WORDS];
    let mut i = 0usize;
    let mut a = 0x9E37_79B1u32;
    while i < WORDS {
        a = a.wrapping_mul(0x0019_660D).wrapping_add(0x3C6E_F35F);
        table[i] = a ^ (i as u32);
        i += 1;
    }
    table
}

/// Walk `TABLE` at `stride_words`, `ACCESSES` times, wrapping.
///
/// The index arithmetic is a mask rather than a modulo: `WORDS` is a power of
/// two, and a `div` in the inner loop would put the divider's cost inside a
/// kernel that exists to measure loads. That is the whole rule of this
/// payload — one cost per kernel.
#[inline(always)]
fn walk(stride_words: usize) -> u32 {
    const MASK: usize = WORDS - 1;
    let mut sum = 0u32;
    let mut index = 0usize;
    let mut n = ACCESSES;
    while n > 0 {
        sum = sum.wrapping_add(core::hint::black_box(&TABLE)[index]);
        index = (index + stride_words) & MASK;
        n -= 1;
    }
    sum
}

pub fn stride_walk_16(_iters: u32) -> u32 {
    walk(16 / 4)
}
pub fn stride_walk_32(_iters: u32) -> u32 {
    walk(32 / 4)
}
pub fn stride_walk_64(_iters: u32) -> u32 {
    walk(64 / 4)
}
pub fn stride_walk_256(_iters: u32) -> u32 {
    walk(256 / 4)
}
pub fn stride_walk_1024(_iters: u32) -> u32 {
    walk(1024 / 4)
}
pub fn stride_walk_4096(_iters: u32) -> u32 {
    walk(4096 / 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_word_count_is_a_power_of_two() {
        assert!(WORDS.is_power_of_two(), "the index mask needs a power of two");
    }

    #[test]
    fn the_table_is_not_uniform() {
        assert_ne!(TABLE[0], TABLE[1]);
        assert_ne!(TABLE[0], TABLE[WORDS - 1]);
    }

    /// Every stride is a whole number of words and smaller than the array,
    /// so each one really does walk the whole thing.
    #[test]
    fn every_stride_fits_the_table() {
        for stride in STRIDES {
            assert_eq!(stride % 4, 0, "stride {stride} is not word-aligned");
            assert!(stride < RODATA_BYTES, "stride {stride} exceeds the table");
        }
    }

    /// Deterministic, which is what lets `acc` be compared structurally
    /// between silicon and an emulator.
    #[test]
    fn the_walks_are_deterministic_and_distinct() {
        let a = stride_walk_16(0);
        assert_eq!(a, stride_walk_16(0));
        assert_ne!(a, stride_walk_4096(0));
    }
}
