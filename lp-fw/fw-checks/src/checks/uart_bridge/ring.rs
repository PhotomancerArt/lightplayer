//! The bounded byte queue one direction of the bridge holds.
//!
//! A bridge is only transparent while both sides keep up. When the sink stalls
//! — a USB host that stopped reading, a UART TX still shifting out the last
//! frame — the source keeps producing, and something has to give. This queue
//! decides *what* gives, and counts it.
//!
//! The rule is **drop the newest, never the oldest**. A bridge that discards
//! the head to make room reorders the stream, and a reordered stream is worse
//! than a truncated one: the reader cannot tell it happened. Dropping the tail
//! leaves a prefix that is byte-exact as far as it goes, and the drop count
//! says how far that was.

/// A fixed-capacity FIFO of bytes with a drop counter.
///
/// `N` is a byte count, not a power-of-two requirement — the indices wrap by
/// remainder, which the compiler turns into a compare-and-subtract for the
/// sizes this payload uses.
#[derive(Debug)]
pub struct ByteRing<const N: usize> {
    buf: [u8; N],
    /// Index of the oldest byte.
    head: usize,
    /// Number of bytes held.
    len: usize,
    /// Bytes offered that did not fit, since construction.
    dropped: u32,
}

impl<const N: usize> Default for ByteRing<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> ByteRing<N> {
    pub const CAPACITY: usize = N;

    pub const fn new() -> Self {
        Self {
            buf: [0u8; N],
            head: 0,
            len: 0,
            dropped: 0,
        }
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub const fn is_full(&self) -> bool {
        self.len == N
    }

    pub const fn free(&self) -> usize {
        N - self.len
    }

    /// Bytes offered but not taken, since construction. Saturating: a bridge
    /// that has dropped four billion bytes has already told you what you need
    /// to know, and wrapping the number would make it lie.
    pub const fn dropped(&self) -> u32 {
        self.dropped
    }

    /// Append as many of `bytes` as fit, in order, counting the rest as
    /// dropped. Returns how many were taken.
    pub fn push(&mut self, bytes: &[u8]) -> usize {
        let take = bytes.len().min(self.free());
        for &b in &bytes[..take] {
            let at = (self.head + self.len) % N;
            self.buf[at] = b;
            self.len += 1;
        }
        let lost = bytes.len() - take;
        if lost > 0 {
            self.dropped = self.dropped.saturating_add(lost as u32);
        }
        take
    }

    /// Copy the oldest bytes into `out` and forget them. Returns how many.
    pub fn pop_into(&mut self, out: &mut [u8]) -> usize {
        let take = out.len().min(self.len);
        for slot in out.iter_mut().take(take) {
            *slot = self.buf[self.head];
            self.head = (self.head + 1) % N;
            self.len -= 1;
        }
        take
    }

    /// Count a loss the ring never saw — a hardware RX FIFO that overran while
    /// this queue was full, say. Kept on the same counter because from a
    /// reader's point of view it is the same hole in the stream.
    pub fn note_dropped(&mut self, bytes: u32) {
        self.dropped = self.dropped.saturating_add(bytes);
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::vec::Vec;

    use super::*;

    #[test]
    fn round_trips_in_order() {
        let mut ring = ByteRing::<8>::new();
        assert_eq!(ring.push(b"abc"), 3);
        let mut out = [0u8; 8];
        assert_eq!(ring.pop_into(&mut out), 3);
        assert_eq!(&out[..3], b"abc");
        assert!(ring.is_empty());
        assert_eq!(ring.dropped(), 0);
    }

    #[test]
    fn wraps_without_reordering() {
        let mut ring = ByteRing::<4>::new();
        let mut out = [0u8; 4];
        let mut seen = Vec::new();
        // Six passes of three bytes through a four-byte ring: every byte fits
        // because the ring is drained each pass, and the wrap happens twice.
        for chunk in [b"abc", b"def", b"ghi", b"jkl", b"mno", b"pqr"] {
            assert_eq!(ring.push(chunk), 3, "{chunk:?} should fit an empty ring");
            let n = ring.pop_into(&mut out);
            seen.extend_from_slice(&out[..n]);
        }
        assert_eq!(seen, b"abcdefghijklmnopqr".to_vec());
        assert_eq!(ring.dropped(), 0);
    }

    #[test]
    fn partial_reads_keep_the_order() {
        let mut ring = ByteRing::<8>::new();
        ring.push(b"12345678");
        let mut two = [0u8; 2];
        let mut seen = Vec::new();
        while ring.pop_into(&mut two) == 2 {
            seen.extend_from_slice(&two);
        }
        assert_eq!(seen, b"12345678".to_vec());
    }

    #[test]
    fn overflow_drops_the_newest_and_counts_it() {
        let mut ring = ByteRing::<4>::new();
        assert_eq!(ring.push(b"abcdef"), 4);
        assert_eq!(ring.dropped(), 2, "`ef` did not fit");
        let mut out = [0u8; 4];
        assert_eq!(ring.pop_into(&mut out), 4);
        assert_eq!(
            &out[..],
            b"abcd",
            "the oldest bytes survive; the tail is what is lost"
        );
    }

    #[test]
    fn a_full_ring_takes_nothing_and_counts_everything() {
        let mut ring = ByteRing::<2>::new();
        ring.push(b"ab");
        assert!(ring.is_full());
        assert_eq!(ring.push(b"cde"), 0);
        assert_eq!(ring.dropped(), 3);
    }

    #[test]
    fn drop_counts_accumulate_across_pushes_and_survive_draining() {
        let mut ring = ByteRing::<2>::new();
        ring.push(b"abcd"); // 2 dropped
        let mut out = [0u8; 2];
        ring.pop_into(&mut out);
        ring.push(b"efg"); // 1 dropped
        assert_eq!(ring.dropped(), 3, "draining is not forgiveness");
    }

    #[test]
    fn noted_losses_join_the_same_counter() {
        let mut ring = ByteRing::<2>::new();
        ring.push(b"abc");
        ring.note_dropped(10);
        assert_eq!(ring.dropped(), 11);
    }

    #[test]
    fn the_counter_saturates_rather_than_wrapping() {
        let mut ring = ByteRing::<2>::new();
        ring.note_dropped(u32::MAX);
        ring.note_dropped(5);
        assert_eq!(ring.dropped(), u32::MAX);
    }
}
