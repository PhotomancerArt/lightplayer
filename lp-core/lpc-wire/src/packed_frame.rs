//! One packed wire message as it goes on a byte stream beside console text:
//! `\n 0x00 'P' COBS(packed) 0x00`.
//!
//! The JSON form of the same message is the line `\nM!{json}\n`; this is its
//! packed twin, and a board writes one or the other per link
//! ([`WireEncoding`](crate::WireEncoding)). The leading `\n` does what it does
//! for a JSON line: it resyncs a reader whose last line was torn, so a frame
//! never splices onto a partial console line. The frame itself is
//! `lp-json-pack`'s ([`lp_json_pack::cobs_frame`]); it contains no `0x00`
//! inside and ends at the next one, so no trailing `\n` is needed.
//!
//! [`ser_packed_frame_to`] builds the whole thing **in place** in one
//! buffer, with no measure pass (plan `lp-json-pack`, G-F2): the packed
//! payload is written at a headroom offset and COBS-encoded forward over it.
//! A message that does not fit, or cannot be packed, is an error the caller
//! answers by writing the JSON line instead.

use lp_json_pack::cobs_frame::{FRAME_KIND_PACK, frame_in_place, in_place_headroom};
use serde::Serialize;

use crate::ser_write::{WireWriteError, ser_wire_to};
use crate::wire_encoding::WireEncoding;

/// Write `value` into `buf` as one packed wire frame (`\n 0x00 'P' COBS
/// 0x00`), returning the bytes written.
///
/// `buf` may be the size of the JSON line budget: a packed frame of a message
/// is never longer than its `\nM!{json}\n` line (asserted on the recorded
/// traffic in this module's tests), so a buffer that holds the JSON line holds
/// the frame. On error `buf` holds garbage.
pub fn ser_packed_frame_to<T: Serialize + ?Sized>(
    buf: &mut [u8],
    value: &T,
) -> Result<usize, WireWriteError> {
    let (lead, body) = buf.split_first_mut().ok_or(WireWriteError::Full)?;
    *lead = b'\n';
    // Room for the frame's growth over the payload, for the largest payload
    // this buffer could hold; a smaller payload needs less.
    let headroom = in_place_headroom(body.len());
    let room = body.get_mut(headroom..).ok_or(WireWriteError::Full)?;
    let n = ser_wire_to(room, WireEncoding::Packed, value)?;
    let framed = frame_in_place(body, FRAME_KIND_PACK, headroom, n).ok_or(WireWriteError::Full)?;
    Ok(1 + framed)
}

/// SPIKE: the COBS frame kind of a learned-table frame.
pub const FRAME_KIND_LEARNED: u8 = b'L';

/// SPIKE: [`ser_packed_frame_to`] for a learned-table frame (`\n 0x00 'L'
/// COBS(header + packed) 0x00`), coded against `dict ++ learned`. On error the
/// table is as it was.
pub fn ser_learned_frame_to<T: Serialize + ?Sized>(
    buf: &mut [u8],
    dict: &'static lp_json_pack::Dictionary,
    learned: &mut dyn lp_json_pack::LearnStore,
    value: &T,
) -> Result<usize, WireWriteError> {
    let (lead, body) = buf.split_first_mut().ok_or(WireWriteError::Full)?;
    *lead = b'\n';
    let headroom = in_place_headroom(body.len());
    let room = body.get_mut(headroom..).ok_or(WireWriteError::Full)?;
    let mark = learned.mark();
    let n = crate::ser_write::ser_learned_to(room, dict, &mut *learned, value)?;
    match frame_in_place(body, FRAME_KIND_LEARNED, headroom, n) {
        Some(framed) => Ok(1 + framed),
        None => {
            learned.truncate(mark);
            Err(WireWriteError::Full)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WireServerMessage;
    use crate::test_traffic::{TrafficDirection, traffic_lines};
    use alloc::vec;
    use lp_json_pack::cobs_frame::cobs_decode;

    /// Every recorded reply frames no longer than its JSON line, so the
    /// firmware's JSON-sized frame buffer holds any packed frame — and the
    /// frame decodes back to the line's JSON byte for byte.
    #[test]
    fn a_packed_frame_is_never_longer_than_its_json_line() {
        let mut replies = 0;
        for line in traffic_lines() {
            if line.direction != TrafficDirection::BoardToHost {
                continue;
            }
            let msg: WireServerMessage = crate::json::from_str(line.json).unwrap();
            let json_line = line.json.len() + 4; // "\nM!" + "\n"
            // Exactly the JSON line's size: the firmware's buffer budget.
            let mut buf = vec![0u8; json_line];
            let n = ser_packed_frame_to(&mut buf, &msg)
                .unwrap_or_else(|e| panic!("line {}: {e}", line.index));
            assert!(n <= json_line, "line {}: {n} > {json_line}", line.index);
            assert_eq!(&buf[..3], b"\n\0P", "line {}", line.index);
            assert_eq!(buf[n - 1], 0, "line {}: closing delimiter", line.index);

            let mut payload = vec![0u8; n];
            let len = cobs_decode(&buf[3..n - 1], &mut payload).unwrap();
            let json = crate::decode_packed_to_json(&payload[..len]).unwrap();
            assert_eq!(json, line.json, "line {}", line.index);
            replies += 1;
        }
        assert!(replies > 100, "the sample's replies were all checked");
    }

    #[test]
    fn a_buffer_too_small_is_full_never_a_panic() {
        let line = traffic_lines()
            .find(|l| l.direction == TrafficDirection::BoardToHost)
            .unwrap();
        let msg: WireServerMessage = crate::json::from_str(line.json).unwrap();
        for size in [0, 1, 2, 3, 8, 32] {
            let mut buf = vec![0u8; size];
            assert_eq!(
                ser_packed_frame_to(&mut buf, &msg),
                Err(WireWriteError::Full),
                "size {size}"
            );
        }
    }
}
