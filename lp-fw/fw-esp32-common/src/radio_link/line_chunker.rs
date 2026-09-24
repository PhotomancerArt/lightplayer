//! Board → host: splitting one framed wire line into notify-sized values.
//!
//! A server frame is serialized once, as `\nM!{json}\n`, into the shared frame
//! buffer (`serial::server_msg`). On USB it goes out as a byte stream. On a
//! radio link it goes out as a sequence of notifications, each at most one
//! ATT value: the negotiated ATT MTU minus the 3-byte notification header,
//! and never more than the characteristic's own capacity. The host re-joins
//! them on newlines, so a chunk boundary means nothing on the wire.

/// ATT notification header: 1-byte opcode + 2-byte handle.
pub const ATT_NOTIFY_HEADER_BYTES: usize = 3;

/// The largest value one notification of `att_mtu` carries, capped at the
/// characteristic's `value_capacity`. Never zero: a link that reports a
/// nonsensical MTU still moves one byte at a time rather than looping forever.
#[must_use]
pub fn notify_payload_len(att_mtu: u16, value_capacity: usize) -> usize {
    usize::from(att_mtu)
        .saturating_sub(ATT_NOTIFY_HEADER_BYTES)
        .min(value_capacity)
        .max(1)
}

/// The `(offset, len)` of each notification value `frame_len` bytes split
/// into, in order.
pub fn chunk_spans(
    frame_len: usize,
    att_mtu: u16,
    value_capacity: usize,
) -> impl Iterator<Item = (usize, usize)> {
    let step = notify_payload_len(att_mtu, value_capacity);
    (0..frame_len)
        .step_by(step)
        .map(move |offset| (offset, step.min(frame_len - offset)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    #[test]
    fn payload_is_mtu_minus_header_capped_at_capacity() {
        assert_eq!(notify_payload_len(23, 244), 20);
        assert_eq!(notify_payload_len(185, 244), 182);
        // The spike's measured MTU: 251 → 248, capped at the 244-B value.
        assert_eq!(notify_payload_len(251, 244), 244);
        assert_eq!(notify_payload_len(0, 244), 1);
    }

    #[test]
    fn spans_cover_the_frame_exactly_in_order() {
        let spans: Vec<_> = chunk_spans(50, 23, 244).collect();
        assert_eq!(spans, vec![(0, 20), (20, 20), (40, 10)]);
        let total: usize = chunk_spans(16_400, 251, 244).map(|(_, len)| len).sum();
        assert_eq!(total, 16_400);
    }

    #[test]
    fn an_empty_frame_has_no_chunks() {
        assert_eq!(chunk_spans(0, 251, 244).count(), 0);
    }
}
