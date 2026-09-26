//! One packed wire message as it goes on a byte stream beside console text:
//! `\n 0x00 'L' COBS(header + packed) 0x00`.
//!
//! The JSON form of the same message is the line `\nM!{json}\n`; this is its
//! packed twin, and a board writes one or the other per link
//! ([`WireEncoding`](crate::WireEncoding)). The leading `\n` does what it does
//! for a JSON line: it resyncs a reader whose last line was torn, so a frame
//! never splices onto a partial console line. The frame itself is
//! `lp-json-pack`'s ([`lp_json_pack::cobs_frame`]) with kind
//! [`FRAME_KIND_LEARNED`]: a learned frame whose payload starts with the
//! link table's 3-byte header (`lp_json_pack::pack_learned`). It contains no
//! `0x00` inside and ends at the next one, so no trailing `\n` is needed.
//!
//! [`ser_learned_frame_to`] builds the whole thing **in place** in one
//! buffer, with no measure pass (plan `lp-json-pack`, G-F2): the packed
//! payload is written at a headroom offset and COBS-encoded forward over it.
//! A message that does not fit, or cannot be packed, is an error the caller
//! answers by writing the JSON line instead, and the table is left as it was.

use lp_json_pack::LearnStore;
use lp_json_pack::cobs_frame::{frame_in_place, in_place_headroom};
use serde::Serialize;

use crate::ser_write::{WireWriteError, ser_learned_to};
use crate::wire_encoding::FRAME_KIND_LEARNED;

/// Write `value` into `buf` as one learned wire frame (`\n 0x00 'L' COBS
/// 0x00`), coded against the link's `table`, and return the bytes written.
///
/// `buf` may be the size of the JSON line budget: a packed frame of a message
/// is never longer than its `\nM!{json}\n` line, first sightings included
/// (asserted on the recorded traffic, replayed in order through one table, in
/// this module's tests), so a buffer that holds the JSON line holds the frame.
/// On error `buf` holds garbage and `table` is as it was; on success the
/// caller rolls `table` back if the frame is then not sent.
pub fn ser_learned_frame_to<T: Serialize + ?Sized>(
    buf: &mut [u8],
    table: &mut dyn LearnStore,
    value: &T,
) -> Result<usize, WireWriteError> {
    let (lead, body) = buf.split_first_mut().ok_or(WireWriteError::Full)?;
    *lead = b'\n';
    // Room for the frame's growth over the payload, for the largest payload
    // this buffer could hold; a smaller payload needs less.
    let headroom = in_place_headroom(body.len());
    let room = body.get_mut(headroom..).ok_or(WireWriteError::Full)?;
    let mark = table.mark();
    let n = ser_learned_to(room, &mut *table, value)?;
    match frame_in_place(body, FRAME_KIND_LEARNED, headroom, n) {
        Some(framed) => Ok(1 + framed),
        None => {
            table.truncate(mark);
            Err(WireWriteError::Full)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WireServerMessage;
    use crate::test_traffic::{TrafficDirection, traffic_lines};
    use crate::wire_encoding::WIRE_SEED;
    use alloc::vec;
    use alloc::vec::Vec;
    use lp_json_pack::cobs_frame::cobs_decode;
    use lp_json_pack::{LearnedTable, decode_learned};

    /// Every recorded reply, in order through one board table (so the first
    /// sighting of every name is covered), frames no longer than its JSON
    /// line, so the firmware's JSON-sized frame buffer holds any packed frame —
    /// and the frame decodes back to the line's JSON byte for byte on a twin
    /// host table.
    #[test]
    fn a_packed_frame_is_never_longer_than_its_json_line() {
        let mut board = LearnedTable::NEW;
        let mut host = LearnedTable::NEW;
        let mut replies = 0;
        for line in traffic_lines() {
            if line.direction != TrafficDirection::BoardToHost {
                continue;
            }
            let msg: WireServerMessage = crate::json::from_str(line.json).unwrap();
            let json_line = line.json.len() + 4; // "\nM!" + "\n"
            // Exactly the JSON line's size: the firmware's buffer budget.
            let mut buf = vec![0u8; json_line];
            let n = ser_learned_frame_to(&mut buf, &mut board, &msg)
                .unwrap_or_else(|e| panic!("line {}: {e}", line.index));
            assert!(n <= json_line, "line {}: {n} > {json_line}", line.index);
            assert_eq!(&buf[..3], b"\n\0L", "line {}", line.index);
            assert_eq!(buf[n - 1], 0, "line {}: closing delimiter", line.index);

            let mut payload = vec![0u8; n];
            let len = cobs_decode(&buf[3..n - 1], &mut payload).unwrap();
            let mut json = Vec::new();
            decode_learned(&WIRE_SEED, &mut host, &payload[..len], &mut json).unwrap();
            assert_eq!(json, line.json.as_bytes(), "line {}", line.index);
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
            let mut table = LearnedTable::NEW;
            assert_eq!(
                ser_learned_frame_to(&mut buf, &mut table, &msg),
                Err(WireWriteError::Full),
                "size {size}"
            );
            assert_eq!(table.mark(), LearnedTable::NEW.mark(), "size {size}");
        }
    }
}
