//! The chunk plan of a deflated push (feature `deflate-plan`, the host's
//! side; never on a device): cut a file into chunks of at most 4 KiB logical
//! whose raw deflate (`miniz_oxide`) fits one record of a board's
//! `record_max`, so each wire chunk is one stored chunk. No hasher: the wire
//! carries no content id (the board computes it), so a client plans with
//! this alone; [`crate::host_deflate_chunks`] adds the ids.
//!
//! The cut is a function of the bytes, `record_max` and the level alone —
//! the same file always cuts the same way, which is what lets an unchanged
//! chunk dedupe against the copy already on the board.

use alloc::vec::Vec;
use core::ops::Range;

use crate::blob_codec::{DEFLATE_PREFIX, MAX_LOGICAL_CHUNK};
use crate::record_header::RECORD_HEADER_LEN;

/// The compression level a push plans with by default (`miniz_oxide`'s
/// best, 10).
pub const DEFAULT_DEFLATE_LEVEL: u8 = 10;

/// One chunk of a planned file: the logical bytes it covers and their raw
/// deflate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedChunk {
    /// The chunk's bytes within the file (its logical offset is
    /// `logical_range.start`).
    pub logical_range: Range<usize>,
    /// Raw deflate (RFC 1951) of those bytes.
    pub deflated: Vec<u8>,
}

/// `bytes` as deflated chunks for a board with `record_max`, at `level`.
///
/// Each chunk is at most [`MAX_LOGICAL_CHUNK`] logical, shrunk in proportion
/// until its deflate fits one record's payload. A chunk that will not shrink
/// (incompressible bytes) is still planned deflated, no smaller than one
/// record's worth of logical bytes; the board stores it as plain bytes. An
/// empty file plans one empty chunk.
pub fn plan_deflated_chunks(bytes: &[u8], record_max: u32, level: u8) -> Vec<PlannedChunk> {
    let room = (record_max - RECORD_HEADER_LEN) as usize - DEFLATE_PREFIX;
    let floor = room + DEFLATE_PREFIX;
    let mut out = Vec::new();
    let mut pos = 0;
    loop {
        let rem = &bytes[pos..];
        let mut n = rem.len().min(MAX_LOGICAL_CHUNK);
        let z = loop {
            let z = miniz_oxide::deflate::compress_to_vec(&rem[..n], level);
            if z.len() <= room || n <= floor.min(rem.len()) {
                break z;
            }
            // Shrink in proportion, never below one stored record's worth.
            let guess = n * room * 15 / 16 / z.len().max(1);
            n = guess.clamp(floor.min(rem.len()), n - 1);
        };
        out.push(PlannedChunk {
            logical_range: pos..pos + n,
            deflated: z,
        });
        pos += n;
        if pos >= bytes.len() {
            return out;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_cover_the_file_in_order_and_each_fits_a_record() {
        let text: Vec<u8> = (0..20_000u32)
            .flat_map(|i| alloc::format!("line {i} of a shader-ish text\n").into_bytes())
            .collect();
        let plan = plan_deflated_chunks(&text, 1024, DEFAULT_DEFLATE_LEVEL);
        let mut at = 0;
        for chunk in &plan {
            assert_eq!(chunk.logical_range.start, at);
            assert!(chunk.logical_range.len() <= MAX_LOGICAL_CHUNK);
            assert!(chunk.deflated.len() + DEFLATE_PREFIX + RECORD_HEADER_LEN as usize <= 1024);
            let mut back = alloc::vec![0u8; chunk.logical_range.len()];
            let n = lp_deflate::inflate(&chunk.deflated, &mut back, 0).unwrap();
            assert_eq!(&back[..n], &text[chunk.logical_range.clone()]);
            at = chunk.logical_range.end;
        }
        assert_eq!(at, text.len());
    }

    #[test]
    fn the_plan_is_a_function_of_the_bytes() {
        let text: Vec<u8> = (0..3_000u32).flat_map(|i| i.to_le_bytes()).collect();
        assert_eq!(
            plan_deflated_chunks(&text, 1024, DEFAULT_DEFLATE_LEVEL),
            plan_deflated_chunks(&text, 1024, DEFAULT_DEFLATE_LEVEL)
        );
    }

    #[test]
    fn an_empty_file_is_one_empty_chunk() {
        let plan = plan_deflated_chunks(&[], 1024, DEFAULT_DEFLATE_LEVEL);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].logical_range, 0..0);
    }
}
