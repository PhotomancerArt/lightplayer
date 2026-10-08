//! A buffer the network asks for after boot, allocated fallibly.
//!
//! The network's buffers that come and go — the relay leg's while it may
//! dial, each path's outgoing frame while it serves the one network session
//! (Wi-Fi relay plan, round 2: the memory fix) — are asked for on a heap a
//! project has already fragmented. An infallible allocation that fails
//! there resets the board; this one says no instead, and the caller turns
//! the work away in words.

use alloc::boxed::Box;
use alloc::vec::Vec;

/// `len` zeroed bytes on the heap, or `None` when the heap cannot give them.
#[must_use]
pub fn try_zeroed_bytes(len: usize) -> Option<Box<[u8]>> {
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(len).ok()?;
    bytes.resize(len, 0);
    Some(bytes.into_boxed_slice())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_gives_zeroed_bytes_and_refuses_what_no_heap_holds() {
        assert_eq!(&*try_zeroed_bytes(3).unwrap(), &[0, 0, 0]);
        assert!(try_zeroed_bytes(usize::MAX / 2).is_none());
    }
}
