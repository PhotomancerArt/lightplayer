//! Host → board: an ATT *long write* (Prepare Write … Execute Write) to the
//! RX characteristic, re-assembled into the bytes it carries.
//!
//! A central may write a value longer than one ATT packet holds as a run of
//! `Prepare Write Request`s (each a segment and its offset into the value)
//! followed by one `Execute Write Request` (flags 1 = write them, 0 = cancel).
//! trouble-host 0.6 answers each prepare by writing the segment into the
//! characteristic at offset 0 and answers the execute with success, but it
//! hands neither to the connection's code as a write: only a plain
//! `Write Request`/`Write Command` reaches `GattEvent::Write`. So before this
//! module the board acknowledged a long write and threw its bytes away, and
//! the page's request was never answered
//! (docs/defects/2026-09-25-a-long-bluetooth-write-is-acknowledged-and-lost.md).
//!
//! The queue is the one piece that is not a radio fact, so it lives here and
//! is host-tested: segments in, the whole value out on execute. Segments must
//! arrive in order and without gaps (offset == bytes queued so far) — what
//! every central does for a long write — and the whole value is bounded by
//! the ATT maximum attribute length (512 B), so a peer cannot grow it past
//! that.

use alloc::vec::Vec;

/// The ATT maximum attribute value length: no long write is longer.
pub const PREPARED_WRITE_MAX: usize = 512;

/// Why a prepared segment was refused; each maps onto an ATT error code the
/// connection answers the central with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrepareRefused {
    /// The segment does not continue the value where it stands
    /// (`INVALID_OFFSET`).
    InvalidOffset,
    /// The value would outgrow [`PREPARED_WRITE_MAX`] (`PREPARE_QUEUE_FULL`).
    QueueFull,
}

/// One connection's pending long write.
#[derive(Debug, Default)]
pub struct PreparedWrite {
    value: Vec<u8>,
}

impl PreparedWrite {
    #[must_use]
    pub const fn new() -> Self {
        Self { value: Vec::new() }
    }

    /// Queue one `Prepare Write Request` segment.
    pub fn prepare(&mut self, offset: u16, segment: &[u8]) -> Result<(), PrepareRefused> {
        if usize::from(offset) != self.value.len() {
            return Err(PrepareRefused::InvalidOffset);
        }
        if self.value.len() + segment.len() > PREPARED_WRITE_MAX {
            return Err(PrepareRefused::QueueFull);
        }
        self.value.extend_from_slice(segment);
        Ok(())
    }

    /// The `Execute Write Request`: flags bit 0 set writes the queued value
    /// (returned, possibly empty), clear cancels it (`None`). Either way the
    /// queue is empty afterwards.
    pub fn execute(&mut self, flags: u8) -> Option<Vec<u8>> {
        let value = core::mem::take(&mut self.value);
        (flags & 0x01 != 0).then_some(value)
    }

    /// Bytes queued and not yet executed.
    #[must_use]
    pub fn queued(&self) -> usize {
        self.value.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_in_order_come_out_whole_on_execute() {
        let line = b"M!{\"id\":7,\"msg\":\"hello\"}\n";
        let mut q = PreparedWrite::new();
        q.prepare(0, &line[..10]).unwrap();
        q.prepare(10, &line[10..20]).unwrap();
        q.prepare(20, &line[20..]).unwrap();
        assert_eq!(q.execute(1).as_deref(), Some(&line[..]));
        assert_eq!(q.queued(), 0);
    }

    #[test]
    fn a_long_write_in_mtu_sized_segments_is_taken() {
        // At the board's ATT MTU of 247 a central sends 242-byte segments
        // (MTU − 5); a long write is up to 512 B.
        let value: Vec<u8> = (0..400u32).map(|i| b'a' + (i % 26) as u8).collect();
        let mut q = PreparedWrite::new();
        q.prepare(0, &value[..242]).unwrap();
        q.prepare(242, &value[242..]).unwrap();
        assert_eq!(q.execute(1), Some(value));
    }

    #[test]
    fn execute_with_flags_zero_cancels() {
        let mut q = PreparedWrite::new();
        q.prepare(0, b"abc").unwrap();
        assert_eq!(q.execute(0), None);
        assert_eq!(q.queued(), 0);
        // …and the next long write starts clean.
        q.prepare(0, b"xyz").unwrap();
        assert_eq!(q.execute(1).as_deref(), Some(&b"xyz"[..]));
    }

    #[test]
    fn a_gap_or_a_repeat_is_an_invalid_offset() {
        let mut q = PreparedWrite::new();
        assert_eq!(q.prepare(3, b"abc"), Err(PrepareRefused::InvalidOffset));
        q.prepare(0, b"abc").unwrap();
        assert_eq!(q.prepare(0, b"abc"), Err(PrepareRefused::InvalidOffset));
        assert_eq!(q.prepare(5, b"abc"), Err(PrepareRefused::InvalidOffset));
        assert_eq!(q.queued(), 3);
    }

    #[test]
    fn the_value_is_bounded_by_the_att_maximum() {
        let mut q = PreparedWrite::new();
        q.prepare(0, &[b'x'; 500]).unwrap();
        assert_eq!(q.prepare(500, &[b'x'; 13]), Err(PrepareRefused::QueueFull));
        q.prepare(500, &[b'x'; 12]).unwrap();
        assert_eq!(q.execute(1).map(|v| v.len()), Some(PREPARED_WRITE_MAX));
    }

    #[test]
    fn an_empty_execute_writes_nothing() {
        let mut q = PreparedWrite::new();
        assert_eq!(q.execute(1), Some(Vec::new()));
    }
}
