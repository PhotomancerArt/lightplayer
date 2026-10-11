//! The server's one fs batch: who owns it, and when it ends.
//!
//! A batch (`FsRequest::BeginBatch` … `CommitBatch`) is the first piece of
//! fs state a server holds between requests — an exception to
//! `file_sync`'s pull-model rule, recorded in
//! `docs/adr/2026-10-10-fs-push-boundary-and-deflated-writes.md`. The
//! filesystem holds the transaction itself (`LpFs::begin_batch`); this holds
//! only its owner and its idle time:
//!
//! - **At most one** is open, **owned by the link that began it**. While it
//!   is open, every other link's fs mutation (a write, a delete, a batch
//!   verb) is refused in words ([`BATCH_BUSY`]); their reads, and everything
//!   not on the fs wire, go on. The server's own writes (a loaded project's
//!   saves, the startup choice a load writes) are never refused: they join
//!   the batch, and land or drop with it.
//! - It **ends** on the owner's `CommitBatch` (the only ending that lands
//!   it) or `AbortBatch`, the owner link closing or its session resetting
//!   ([`BatchState::link_ended`]), a second `BeginBatch` from the owner (a
//!   client that lost the first answer starts again), or
//!   [`BATCH_IDLE_TIMEOUT_MS`] with no request from the owner.
//! - **Idle time excludes handling time.** A tick's `delta_ms` covers the
//!   tick before it, so after a tick that handled a request — a
//!   `LoadProject` compiling for twenty seconds on a C6 — the next delta is
//!   that request's time and is not counted. Only ticks that followed a
//!   tick with nothing to answer add to the idle time.
//! - A batch dropped by the timeout leaves its owner's next fs mutation
//!   refused, naming why, until that link sends `BeginBatch` or
//!   `AbortBatch`: a slow client must not go on writing outside the batch
//!   it thinks it holds. A link that closed or reset is a new session and
//!   is not held to its old one's batch.
//!
//! A backend without transactions (`LpFs::batches_are_atomic` is `false`)
//! never opens one: `BeginBatch` answers `atomic: false`, and the client
//! runs the two-slot push.

extern crate alloc;

use alloc::format;
use alloc::string::String;

use lpc_shared::transport::LinkId;
use lpc_wire::server::{BatchOp, FsRequest, FsResponse};
use lpfs::LpFs;

/// How long an open batch may go without a request from its owner (handling
/// time excluded) before the server drops it.
pub const BATCH_IDLE_TIMEOUT_MS: u64 = 60_000;

/// What another link's fs mutation is told while a batch is open.
pub const BATCH_BUSY: &str =
    "batch busy: another connection is sending a project to this board; try again shortly";

/// What a batch's owner is told after the idle timeout dropped it.
const BATCH_TIMED_OUT: &str = "the batch was dropped: 60 s passed with no request from this \
     connection, so nothing it sent since BeginBatch was kept; abort and start again";

/// The server's batch, when one is open.
#[derive(Debug, Default)]
pub struct BatchState {
    open: Option<OpenBatch>,
    /// The link whose batch the idle timeout dropped, until it begins or
    /// aborts again.
    timed_out: Option<LinkId>,
    /// The tick that is ending handled a request: the next tick's delta is
    /// that request's time, not idle time.
    handled_this_tick: bool,
}

#[derive(Debug)]
struct OpenBatch {
    owner: LinkId,
    idle_ms: u64,
}

impl BatchState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The open batch's owner, if one is open.
    pub fn owner(&self) -> Option<LinkId> {
        self.open.as_ref().map(|open| open.owner)
    }

    /// The start of a tick: count `delta_ms` as idle time unless the last
    /// tick handled a request, and drop the batch past the timeout. `true`
    /// if it dropped one.
    pub fn tick(&mut self, delta_ms: u32, fs: &dyn LpFs) -> bool {
        let skip = core::mem::take(&mut self.handled_this_tick);
        let Some(open) = self.open.as_mut() else {
            return false;
        };
        if !skip {
            open.idle_ms = open.idle_ms.saturating_add(u64::from(delta_ms));
        }
        if open.idle_ms < BATCH_IDLE_TIMEOUT_MS {
            return false;
        }
        let owner = open.owner;
        log::warn!(
            "fs batch of link {owner}: dropped after {} ms with no request",
            open.idle_ms
        );
        self.drop_open(fs);
        self.timed_out = Some(owner);
        true
    }

    /// `link` closed, or its session reset: a batch it owns is dropped (its
    /// session is gone), and it is no longer held to a timed-out one. `true`
    /// if it dropped one.
    pub fn link_ended(&mut self, link: LinkId, fs: &dyn LpFs) -> bool {
        if self.timed_out == Some(link) {
            self.timed_out = None;
        }
        if self.owner() != Some(link) {
            return false;
        }
        log::info!("fs batch of link {link}: dropped, the link's session ended");
        self.drop_open(fs);
        true
    }

    /// A request from `link` was handled this tick (any request: its
    /// handling time is excluded from the idle time either way); one from
    /// the owner also resets the idle time.
    pub fn note_handled(&mut self, link: LinkId) {
        self.handled_this_tick = true;
        if let Some(open) = self.open.as_mut()
            && open.owner == link
        {
            open.idle_ms = 0;
        }
    }

    /// The refusal `link`'s fs request gets, if it gets one: a mutation from
    /// a link that does not own the open batch ([`BATCH_BUSY`]), or a
    /// mutation (or commit) from the owner of a batch the timeout dropped.
    /// `None`: the request goes ahead.
    pub fn refusal(&self, link: LinkId, request: &FsRequest) -> Option<FsResponse> {
        if !fs_request_changes_files(request) {
            return None;
        }
        if let Some(owner) = self.owner()
            && owner != link
        {
            return Some(refused(request, String::from(BATCH_BUSY)));
        }
        let restarts = matches!(request, FsRequest::BeginBatch | FsRequest::AbortBatch);
        if self.timed_out == Some(link) && !restarts {
            return Some(refused(request, String::from(BATCH_TIMED_OUT)));
        }
        None
    }

    /// Answer a batch verb from `link` (after [`Self::refusal`] let it
    /// through). Any other request is not a batch verb and is answered with
    /// an error saying so.
    pub fn handle(&mut self, link: LinkId, request: &FsRequest, fs: &dyn LpFs) -> FsResponse {
        let atomic = fs.batches_are_atomic();
        let answer = |op, error: Option<String>| FsResponse::Batch { op, atomic, error };
        match request {
            FsRequest::BeginBatch => {
                self.timed_out = None;
                if !atomic {
                    // Nothing to open: every write commits by itself.
                    return answer(BatchOp::Begin, None);
                }
                if self.owner() == Some(link) {
                    // The owner lost the first answer and began again.
                    log::info!("fs batch of link {link}: begun again; the old one is dropped");
                    self.drop_open(fs);
                }
                match fs.begin_batch() {
                    Ok(()) => {
                        self.open = Some(OpenBatch {
                            owner: link,
                            idle_ms: 0,
                        });
                        answer(BatchOp::Begin, None)
                    }
                    Err(error) => answer(BatchOp::Begin, Some(format!("{error}"))),
                }
            }
            FsRequest::CommitBatch => {
                if self.owner() != Some(link) {
                    return answer(BatchOp::Commit, Some(String::from("no batch is open")));
                }
                self.open = None;
                match fs.commit_batch() {
                    Ok(()) => answer(BatchOp::Commit, None),
                    Err(error) => {
                        // The trait leaves a failed commit open; the server
                        // drops it, so the board is never left holding it.
                        let _ = fs.abort_batch();
                        answer(
                            BatchOp::Commit,
                            Some(format!(
                                "the batch could not be committed and was dropped: {error}"
                            )),
                        )
                    }
                }
            }
            FsRequest::AbortBatch => {
                self.timed_out = None;
                if self.owner() == Some(link) {
                    self.open = None;
                    if let Err(error) = fs.abort_batch() {
                        return answer(BatchOp::Abort, Some(format!("{error}")));
                    }
                }
                // With none open, there is nothing to drop.
                answer(BatchOp::Abort, None)
            }
            _ => answer(
                BatchOp::Abort,
                Some(String::from("not a batch verb")),
            ),
        }
    }

    fn drop_open(&mut self, fs: &dyn LpFs) {
        if self.open.take().is_some()
            && let Err(error) = fs.abort_batch()
        {
            log::warn!("fs batch: the abort failed: {error}");
        }
    }
}

/// Whether `request` may change what the filesystem holds: every write and
/// delete, and every batch verb (a begin may drop an open batch, a commit
/// lands one, an abort drops one). Exhaustive, with no wildcard: a new fs
/// request does not compile until it is placed here. The server refuses
/// these from a link that does not own the open batch, and re-reads its
/// cached device store after any of them.
pub fn fs_request_changes_files(request: &FsRequest) -> bool {
    match request {
        FsRequest::Read { .. }
        | FsRequest::ListDir { .. }
        | FsRequest::ChangesSince { .. }
        | FsRequest::HashPackage { .. } => false,
        FsRequest::Write { .. }
        | FsRequest::WriteChunk { .. }
        | FsRequest::WriteChunkDeflated { .. }
        | FsRequest::DeleteFile { .. }
        | FsRequest::DeleteDir { .. }
        | FsRequest::BeginBatch
        | FsRequest::CommitBatch
        | FsRequest::AbortBatch => true,
    }
}

/// `request`'s own response shape, carrying `error`.
fn refused(request: &FsRequest, error: String) -> FsResponse {
    let error = Some(error);
    match request {
        FsRequest::Write { path, .. } => FsResponse::Write {
            path: path.clone(),
            error,
        },
        FsRequest::WriteChunk { path, offset, .. }
        | FsRequest::WriteChunkDeflated { path, offset, .. } => FsResponse::WriteChunk {
            path: path.clone(),
            offset: *offset,
            written: 0,
            error,
        },
        FsRequest::DeleteFile { path } => FsResponse::DeleteFile {
            path: path.clone(),
            error,
        },
        FsRequest::DeleteDir { path } => FsResponse::DeleteDir {
            path: path.clone(),
            error,
        },
        FsRequest::BeginBatch => FsResponse::Batch {
            op: BatchOp::Begin,
            atomic: false,
            error,
        },
        FsRequest::CommitBatch => FsResponse::Batch {
            op: BatchOp::Commit,
            atomic: false,
            error,
        },
        FsRequest::AbortBatch => FsResponse::Batch {
            op: BatchOp::Abort,
            atomic: false,
            error,
        },
        // Reads are never refused (see `fs_request_changes_files`); a
        // refusal of one would be a bug, said in the read's own shape.
        FsRequest::Read { path } => FsResponse::Read {
            path: path.clone(),
            data: None,
            error,
        },
        FsRequest::ListDir { path, .. } => FsResponse::ListDir {
            path: path.clone(),
            entries: alloc::vec::Vec::new(),
            error,
        },
        FsRequest::ChangesSince { .. } => FsResponse::Changes {
            entries: alloc::vec::Vec::new(),
            next: None,
            version: None,
            error,
        },
        FsRequest::HashPackage { prefix } => FsResponse::PackageHash {
            prefix: prefix.clone(),
            hash: String::new(),
            error,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::AsLpPathBuf;

    #[test]
    fn every_fs_request_is_placed_as_a_change_or_not() {
        let path = "/projects/a/f".as_path_buf();
        let reads = [
            FsRequest::Read { path: path.clone() },
            FsRequest::ListDir {
                path: path.clone(),
                recursive: false,
            },
            FsRequest::ChangesSince {
                prefix: path.clone(),
                since: lpc_model::FsVersion::new(0),
                cursor: None,
            },
            FsRequest::HashPackage {
                prefix: path.clone(),
            },
        ];
        for request in &reads {
            assert!(!fs_request_changes_files(request), "{request:?}");
        }
        let changes = [
            FsRequest::Write {
                path: path.clone(),
                data: alloc::vec![],
            },
            FsRequest::WriteChunk {
                path: path.clone(),
                offset: 0,
                data: alloc::vec![],
            },
            FsRequest::WriteChunkDeflated {
                path: path.clone(),
                offset: 0,
                logical_len: 0,
                data: alloc::vec![],
            },
            FsRequest::DeleteFile { path: path.clone() },
            FsRequest::DeleteDir { path },
            FsRequest::BeginBatch,
            FsRequest::CommitBatch,
            FsRequest::AbortBatch,
        ];
        for request in &changes {
            assert!(fs_request_changes_files(request), "{request:?}");
        }
    }
}
