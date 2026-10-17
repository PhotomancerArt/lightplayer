//! Decoding one `Blob` payload. The device never encodes: its own writes are
//! stored, and host-deflated chunks (`put_chunk_deflated`) arrive coded.
//! Decoding is `lp_deflate::inflate_with`, the device's own decoder, straight
//! into the caller's buffer (no second chunk-sized buffer), with its Huffman
//! tables on the heap ([`heap_tables`]).

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::record_kind::ChunkCodec;

/// The most logical bytes one chunk decodes to.
pub const MAX_LOGICAL_CHUNK: usize = 4096;

/// The payload bytes before the deflate stream.
pub const DEFLATE_PREFIX: usize = 2;

/// A coded payload's logical length (`None` = malformed).
pub fn logical_len(codec: ChunkCodec, payload: &[u8]) -> Option<usize> {
    match codec {
        ChunkCodec::Stored => Some(payload.len()),
        ChunkCodec::Deflate => {
            let len = usize::from(u16::from_le_bytes([*payload.first()?, *payload.get(1)?]));
            (len <= MAX_LOGICAL_CHUNK).then_some(len)
        }
    }
}

/// Append the decoded bytes of `payload` to `out`. `None` = undecodable
/// (`out` is left as it was).
pub fn decode_blob_into(codec: ChunkCodec, payload: &[u8], out: &mut Vec<u8>) -> Option<()> {
    let len = logical_len(codec, payload)?;
    let start = out.len();
    match codec {
        ChunkCodec::Stored => out.extend_from_slice(payload),
        ChunkCodec::Deflate => {
            out.resize(start + len, 0);
            let stream = payload.get(DEFLATE_PREFIX..)?;
            match lp_deflate::inflate_with(stream, &mut out[start..], 0, &mut heap_tables()) {
                Ok(n) if n == len => {}
                _ => {
                    out.truncate(start);
                    return None;
                }
            }
        }
    }
    Some(())
}

/// The inflater's Huffman tables (≈ 3.8 KiB), on the heap. A read inflates
/// a chunk from deep inside a project load, and on the C6 the stack frame
/// `lp_deflate::inflate` keeps them in cost the main stack 3.6–5.6 KB of its
/// 12 KiB margin (`lp-cli/tests/emu_stack_peak.rs`, the tree-store default
/// switch); the heap has the room for a transient table.
pub(crate) fn heap_tables() -> Box<lp_deflate::Tables> {
    // SAFETY: `lp_deflate::Tables` holds integer arrays only, and all-zero
    // bytes are a valid value (its docs): every table is rebuilt before it
    // is read. Zeroed on the heap, so no 3.8 KiB value is built on the stack
    // first.
    unsafe { Box::<lp_deflate::Tables>::new_zeroed().assume_init() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn stored_and_deflate_decode_and_garbage_never_panics() {
        let mut out = vec![9u8];
        decode_blob_into(ChunkCodec::Stored, b"abc", &mut out).unwrap();
        assert_eq!(out, b"\x09abc");
        // A stored deflate block: BFINAL=1, BTYPE=00, LEN=3, NLEN, "xyz".
        let z = [3, 0, 0x01, 0x03, 0x00, 0xFC, 0xFF, b'x', b'y', b'z'];
        decode_blob_into(ChunkCodec::Deflate, &z, &mut out).unwrap();
        assert_eq!(out, b"\x09abcxyz");
        let mut wrong_len = z;
        wrong_len[0] = 4;
        assert!(decode_blob_into(ChunkCodec::Deflate, &wrong_len, &mut out).is_none());
        assert_eq!(out.len(), 7, "a failed decode leaves the buffer as it was");
        for n in 0..40u8 {
            let junk: Vec<u8> = (0..n).map(|i| i.wrapping_mul(37)).collect();
            let _ = decode_blob_into(ChunkCodec::Deflate, &junk, &mut out);
        }
    }
}
