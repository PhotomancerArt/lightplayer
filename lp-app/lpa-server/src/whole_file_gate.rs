//! The whole-file read gate: one rule for every request that reads a file
//! into one contiguous heap block.
//!
//! A whole-file read is one allocation of the file's size, and on a board
//! with a project loaded and a radio link open that is often more than the
//! largest free block (a 10,240 B read reset the silicon C6, PR B's desk
//! walk; `docs/defects/2026-10-06-a-message-the-heap-cannot-reassemble-resets-the-board.md`).
//! `FsRequest::Read` (`handlers::fs_read_refusal`), Studio's pull,
//! `FsRequest::ChangesSince`, and the package hash Studio's Edit press asks
//! for first, `FsRequest::HashPackage` (both `file_sync`), read files whole,
//! so each asks this one question first and refuses in the same words —
//! refusal, not reset.

extern crate alloc;

use alloc::format;
use alloc::string::String;

use lpc_model::LpPath;
use lpfs::LpFs;

use crate::server::ReadHeadroomProbe;

/// Bytes past a file's own size a read needs in one block: its `Vec`'s
/// slack and the reply's other fields. The reply's base64 is written into the
/// static frame buffer, not the heap.
pub const FS_READ_SLACK_BYTES: u64 = 512;

/// Why reading `path` whole would not fit the heap's largest free block, in
/// the words the client shows; `None` when it fits, when nothing probes the
/// heap, or when the file's size cannot be told (the read reports its own
/// error).
pub fn whole_file_refusal(
    fs: &dyn LpFs,
    path: &LpPath,
    probe: Option<ReadHeadroomProbe>,
) -> Option<String> {
    let largest = u64::from(probe.and_then(|probe| probe())?);
    let size = fs.file_size(path).ok()?;
    let needs = size + FS_READ_SLACK_BYTES;
    if largest >= needs {
        return None;
    }
    Some(format!(
        "read refused: board memory busy (largest block {largest} B; a {size} B file needs \
         {needs} B); retry shortly"
    ))
}
