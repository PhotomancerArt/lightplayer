//! Test messages that carry their own identity and a checkable body, so the
//! receiving side can tell a duplicate, a gap, a reordering or damage that
//! slipped past the checksum.
//!
//! Layout: `inc:u16 ‖ gen:u32 ‖ idx:u32 ‖ sent_at:u64 ‖ filler`, where `inc`
//! numbers the sender's incarnation (a reboot), `gen` its link generation at
//! send time and `idx` the message within that generation.

use std::vec::Vec;

use crate::Micros;

pub const PROBE_HEADER: usize = 18;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Probe {
    pub inc: u16,
    pub gen_: u32,
    pub idx: u32,
    pub sent_at: Micros,
}

impl Probe {
    /// `size` bytes (at least [`PROBE_HEADER`]).
    pub fn encode(&self, size: usize) -> Vec<u8> {
        let mut v = Vec::with_capacity(size.max(PROBE_HEADER));
        v.extend_from_slice(&self.inc.to_le_bytes());
        v.extend_from_slice(&self.gen_.to_le_bytes());
        v.extend_from_slice(&self.idx.to_le_bytes());
        v.extend_from_slice(&self.sent_at.to_le_bytes());
        for i in PROBE_HEADER..size {
            v.push(self.filler(i));
        }
        v
    }

    /// `None` if the body does not match its header (undetected damage).
    pub fn decode(data: &[u8]) -> Option<Probe> {
        if data.len() < PROBE_HEADER {
            return None;
        }
        let p = Probe {
            inc: u16::from_le_bytes(data[0..2].try_into().ok()?),
            gen_: u32::from_le_bytes(data[2..6].try_into().ok()?),
            idx: u32::from_le_bytes(data[6..10].try_into().ok()?),
            sent_at: u64::from_le_bytes(data[10..18].try_into().ok()?),
        };
        let ok = data[PROBE_HEADER..]
            .iter()
            .enumerate()
            .all(|(i, &b)| b == p.filler(i + PROBE_HEADER));
        ok.then_some(p)
    }

    fn filler(&self, i: usize) -> u8 {
        let x = (self.idx as u64)
            ^ ((self.gen_ as u64) << 20)
            ^ ((self.inc as u64) << 40)
            ^ self.sent_at;
        let x = x
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add((i as u64).wrapping_mul(0xD6E8_FEB8_6659_FD93));
        (x >> 56) as u8
    }
}
