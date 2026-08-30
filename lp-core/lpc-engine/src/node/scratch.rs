//! Fallible sizing for persistent tick-path scratch buffers.

use alloc::vec::Vec;

use super::NodeError;

/// Size `scratch` to exactly `len` elements, allocating **fallibly**.
///
/// Tick-path buffers must never grow through the infallible-alloc route: on
/// device, a failed infallible allocation stages an OOM reset, so a transient
/// heap squeeze mid-frame becomes a reboot
/// (`docs/defects/2026-08-29-flash-write-wedges-under-zook-playback.md`).
/// Failing here surfaces a [`NodeError`] instead — the frame's product
/// degrades, the board stays up, and the next tick retries.
///
/// Capacity is kept when `len` shrinks: these are per-node scratches whose
/// steady-state size is the node's own, so the buffer settles at its
/// high-water mark — one allocation for the life of the node, zero per tick.
pub(crate) fn ensure_scratch_len<T: Clone + Default>(
    scratch: &mut Vec<T>,
    len: usize,
    what: &'static str,
) -> Result<(), NodeError> {
    if scratch.len() >= len {
        scratch.truncate(len);
        return Ok(());
    }
    scratch.try_reserve(len - scratch.len()).map_err(|_| {
        NodeError::msg(alloc::format!(
            "{what}: {len}-element scratch allocation failed; skipping frame"
        ))
    })?;
    scratch.resize(len, T::default());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_scratch_len_sizes_exactly_and_keeps_capacity() {
        let mut scratch: Vec<u16> = Vec::new();
        ensure_scratch_len(&mut scratch, 8, "test").unwrap();
        assert_eq!(scratch.len(), 8);

        scratch.fill(7);
        ensure_scratch_len(&mut scratch, 3, "test").unwrap();
        assert_eq!(scratch.len(), 3);
        assert!(
            scratch.capacity() >= 8,
            "shrinking must keep the high-water capacity"
        );

        ensure_scratch_len(&mut scratch, 8, "test").unwrap();
        assert_eq!(scratch.len(), 8);
        // Regrown elements past the truncation point are default-filled.
        assert_eq!(&scratch[..3], &[7, 7, 7]);
    }
}
