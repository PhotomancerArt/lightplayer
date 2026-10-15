//! The push conversation, in one runtime-neutral place.
//!
//! Studio has had these steps since the sim shipped — they are what
//! `StudioServerClient::open_library_project` does — but they lived above a
//! `StudioServerClient`, which owns a session, a log sink and a pull loop.
//! A device push has none of those: it runs inside a coarse effect that
//! borrowed one serial port for the duration, on whatever `ClientIo` that
//! platform has (Web Serial line framing in the browser, the fake device's
//! byte stream on the host). So the conversation lives here, over the plain
//! [`LpClient`], and both callers run the SAME one.
//!
//! ```text
//! 1. ListLoadedProjects  → which storage dir does this board run from?
//! 2. StopAll → BeginBatch → does this board's filesystem have batches?
//! 3. atomic:     one slot  (crate::device_push_one_slot)
//!    not atomic: two slots (crate::device_push_two_slot, delete at adoption)
//!    both: clear → writes (deflated where that shrinks them) → LoadProject
//!          → HashPackage, through the shared primitive (crate::push_files)
//! ```
//!
//! Step 1 is why the push is not simply "write to `demo`": a board flashed
//! by the CLI, or by an older Studio, runs from a dir of its own, and
//! writing beside it would leave two projects on a device that loads one.
//! Replacing the dir it ALREADY runs from is what makes a push idempotent.
//! A board running nothing is asked which dir it BOOTS (`/lightplayer.json`):
//! one that refused its saved project still holds it, and that is the dir
//! the push replaces.
//!
//! Step 2 asks the board, not a build fact: the `BeginBatch` answer's
//! `atomic` is what the filesystem under this request does
//! (`docs/adr/2026-10-10-fs-push-boundary-and-deflated-writes.md`). A board
//! with batches replaces the project in its own folder and commits after the
//! load and the hash; any failure aborts, and the old project is whole. A
//! board without them opens nothing and gets the two-slot push.
//!
//! The hash is not optional. A serial wire drops bytes; a truncated write
//! that loaded anyway would leave a board running something no library has,
//! and the next sync verdict would be computed against a lie.

use crate::client::LpClient;
use crate::client_error::{ClientError, ClientResult};
use crate::client_io::ClientIo;
use crate::push_files::{LpClientSink, abort_batch, begin_batch};

/// What a finished push did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PushReport {
    /// The project storage dir under `/projects/` the bytes went to.
    pub storage_id: String,
    /// The package hash the device reported afterwards — equal to the
    /// expected one, or this would have been an error.
    pub hash: String,
}

/// Progress callback: a label, and a percent when there is an honest one.
pub type PushProgress<'a> = &'a mut dyn FnMut(String, Option<u8>);

/// Run the push conversation against a device that is already listening.
///
/// `fallback_storage_id` is used only when the board reports nothing loaded
/// AND boots no saved folder (`/lightplayer.json`) — a freshly flashed
/// board, which has no dir to replace. A board that boots dark (it refused
/// its saved folder) has that folder replaced, the same way a running one
/// does.
pub async fn push_project<Io: ClientIo>(
    client: &mut LpClient<Io>,
    files: &[(String, Vec<u8>)],
    expected_hash: &str,
    fallback_storage_id: &str,
    progress: PushProgress<'_>,
) -> ClientResult<PushReport> {
    progress("Asking the board what it is running".to_string(), Some(5));
    let loaded = client.project_list_loaded().await?;
    let running = loaded
        .value
        .first()
        .and_then(|project| storage_id_of(project.path.as_str()));
    // A board running nothing may still hold the project it boots — one it
    // refused (an old format, a heap gate). That folder is what this push
    // replaces, exactly as it replaces a running one; writing the fallback
    // beside it would leave the refused copy on flash for good.
    let previous = match &running {
        Some(running) => Some(running.clone()),
        None => saved_startup_project(client).await,
    };

    // Stopped before the batch opens, so nothing the running project saves
    // on its way out joins it.
    client.stop_all_projects().await?;
    let atomic = match begin_batch(&mut LpClientSink::new(client)).await {
        Ok(atomic) => atomic,
        Err(error) => {
            // The begin may have reached the board even if its answer did
            // not: drop whatever it opened.
            let _ = abort_batch(&mut LpClientSink::new(client)).await;
            return Err(error);
        }
    };
    match atomic {
        true => {
            crate::device_push_one_slot::push_one_slot(
                client,
                files,
                expected_hash,
                fallback_storage_id,
                running,
                previous,
                progress,
            )
            .await
        }
        false => {
            crate::device_push_two_slot::push_two_slot(
                client,
                files,
                expected_hash,
                fallback_storage_id,
                running,
                previous,
                progress,
            )
            .await
        }
    }
}

/// Check the board holds exactly the library's bytes in `storage_id`.
pub(crate) async fn verify_hash<Io: ClientIo>(
    client: &mut LpClient<Io>,
    storage_id: &str,
    expected_hash: &str,
) -> ClientResult<String> {
    let outcome = client.hash_package(storage_id).await?;
    match outcome.value == expected_hash {
        true => Ok(outcome.value),
        // The bytes on the board are not the bytes in the library.
        // Saying so beats a green card over a project nobody has.
        false => Err(ClientError::Protocol(format!(
            "the board ended up with different bytes than the library sent \
             (device {}, library {expected_hash}) — the project was not \
             fully written",
            outcome.value
        ))),
    }
}

/// Whether the board said its flash is full. Matched on the error text, the
/// way [`LpClient::delete_project_dir`] matches a missing dir: fs errors
/// cross the wire as display strings (littlefs's words, which the tree
/// store's adapter uses too).
pub(crate) fn is_no_space(error: &ClientError) -> bool {
    match error {
        ClientError::Server(message) | ClientError::Protocol(message) => {
            message.contains("no space left on device")
        }
        _ => false,
    }
}

pub(crate) fn with_restore_note(error: ClientError, old: &str, restored: bool) -> ClientError {
    let note = match restored {
        true => format!("the board is running its previous project ({old}) again"),
        false => format!(
            "the board could not reload its previous project ({old}) either; \
             it is still on the board and loads at the next boot"
        ),
    };
    with_note(error, &note)
}

pub(crate) fn with_note(error: ClientError, note: &str) -> ClientError {
    match error {
        ClientError::Server(message) => ClientError::Server(format!("{message} — {note}")),
        ClientError::Protocol(message) => ClientError::Protocol(format!("{message} — {note}")),
        other => other,
    }
}

/// The folder a board boots (`/lightplayer.json`'s `startup_project`), when
/// that folder is on the board. Best-effort: a board with no config, an
/// unreadable one (a link below the edit tier may not read outside
/// `/projects/`), or one naming a folder that is gone has nothing to
/// replace, and the push falls back as it always did.
///
/// Shared with [`crate::device_remove::remove_project`]: a dark board's
/// fallback slot, there as here, is only for a board with no saved folder
/// at all — a freshly flashed one. A board that boots dark because it
/// refused a saved folder has that folder removed too, the same one a push
/// would replace.
pub(crate) async fn saved_startup_project<Io: ClientIo>(
    client: &mut LpClient<Io>,
) -> Option<String> {
    use lpc_model::AsLpPathBuf;
    use lpc_model::server::server_config::ServerConfig;

    let config = ServerConfig::PATH.as_path_buf();
    let bytes = client.fs_read(config.as_path()).await.ok()?.value;
    let name = lpc_wire::json::from_slice::<ServerConfig>(&bytes)
        .ok()?
        .startup_project?;
    // Only a plain folder name under `/projects/` is one a push may replace.
    let id = storage_id_of(&format!("/projects/{name}")).filter(|id| *id == name)?;
    let projects = "/projects".as_path_buf();
    let on_board = client
        .fs_list_dir(projects.as_path(), false)
        .await
        .ok()?
        .value
        .iter()
        .any(|path| storage_id_of(path.as_str()).as_deref() == Some(id.as_str()));
    on_board.then_some(id)
}

/// The storage dir name inside a reported project path (`/projects/demo` →
/// `demo`). `None` for a path that is not under `/projects/`, which is not a
/// dir a push may replace — nor one a removal may delete, which is why the
/// remove conversation reads the board's report through this same function.
pub(crate) fn storage_id_of(path: &str) -> Option<String> {
    let rest = path
        .trim_start_matches('/')
        .strip_prefix("projects/")?
        .trim_matches('/');
    let id = rest.split('/').next()?;
    (!id.is_empty()).then(|| id.to_string())
}

/// The scripted board both conversations' tests talk to.
#[cfg(test)]
pub(crate) mod test_script {
    use lpc_model::AsLpPathBuf;
    use lpc_wire::{BatchOp, WireServerMessage, WireServerMsgBody};

    pub(crate) const HASH: &str = "abc123";
    pub(crate) const NO_SPACE: &str = "no space left on device";

    pub(crate) fn files() -> Vec<(String, Vec<u8>)> {
        vec![
            ("project.json".to_string(), b"{}".to_vec()),
            ("main.glsl".to_string(), b"void main() {}".to_vec()),
        ]
    }

    pub(crate) fn loaded(id: u64, dir: &str) -> WireServerMessage {
        let project = lpc_wire::LoadedProject::new(
            lpc_wire::WireProjectHandle(1),
            format!("/projects/{dir}").as_path_buf(),
        );
        WireServerMessage::new(
            id,
            WireServerMsgBody::ListLoadedProjects {
                projects: vec![project],
            },
        )
    }

    pub(crate) fn nothing_loaded(id: u64) -> WireServerMessage {
        WireServerMessage::new(
            id,
            WireServerMsgBody::ListLoadedProjects {
                projects: Vec::new(),
            },
        )
    }

    pub(crate) fn read_missing(id: u64) -> WireServerMessage {
        WireServerMessage::new(
            id,
            WireServerMsgBody::Filesystem(lpc_wire::FsResponse::Read {
                path: "/lightplayer.json".as_path_buf(),
                data: None,
                error: Some("File not found: /lightplayer.json".to_string()),
            }),
        )
    }

    pub(crate) fn stopped(id: u64) -> WireServerMessage {
        WireServerMessage::new(id, WireServerMsgBody::StopAllProjects)
    }

    pub(crate) fn batch(id: u64, op: BatchOp, atomic: bool) -> WireServerMessage {
        WireServerMessage::new(
            id,
            WireServerMsgBody::Filesystem(lpc_wire::FsResponse::Batch {
                op,
                atomic,
                error: None,
            }),
        )
    }

    pub(crate) fn deleted(id: u64) -> WireServerMessage {
        WireServerMessage::new(
            id,
            WireServerMsgBody::Filesystem(lpc_wire::FsResponse::DeleteDir {
                path: "/projects/x".as_path_buf(),
                error: None,
            }),
        )
    }

    pub(crate) fn written(id: u64, error: Option<&str>) -> WireServerMessage {
        WireServerMessage::new(
            id,
            WireServerMsgBody::Filesystem(lpc_wire::FsResponse::Write {
                path: "/projects/x/f".as_path_buf(),
                error: error.map(str::to_string),
            }),
        )
    }

    pub(crate) fn loaded_ok(id: u64) -> WireServerMessage {
        WireServerMessage::new(
            id,
            WireServerMsgBody::LoadProject {
                handle: lpc_wire::WireProjectHandle(2),
            },
        )
    }

    pub(crate) fn server_error(id: u64, error: &str) -> WireServerMessage {
        WireServerMessage::new(
            id,
            WireServerMsgBody::Error {
                error: error.to_string(),
            },
        )
    }

    pub(crate) fn hashed(id: u64, hash: &str) -> WireServerMessage {
        WireServerMessage::new(
            id,
            WireServerMsgBody::Filesystem(lpc_wire::FsResponse::PackageHash {
                prefix: "/projects/x".as_path_buf(),
                hash: hash.to_string(),
                error: None,
            }),
        )
    }

    /// What the conversation asked, one word-and-target per request.
    pub(crate) fn ops(sent: &[lpc_wire::ClientMessage]) -> Vec<String> {
        use lpc_wire::{ClientRequest, FsRequest};
        sent.iter()
            .map(|message| match &message.msg {
                ClientRequest::ListLoadedProjects => "list-loaded".to_string(),
                ClientRequest::StopAllProjects => "stop".to_string(),
                ClientRequest::Filesystem(FsRequest::BeginBatch) => "begin".to_string(),
                ClientRequest::Filesystem(FsRequest::CommitBatch) => "commit".to_string(),
                ClientRequest::Filesystem(FsRequest::AbortBatch) => "abort".to_string(),
                ClientRequest::Filesystem(FsRequest::DeleteDir { path }) => {
                    format!("delete {}", path.as_str().trim_start_matches("/projects/"))
                }
                ClientRequest::Filesystem(FsRequest::Write { path, .. }) => {
                    format!("write {}", path.as_str())
                }
                ClientRequest::Filesystem(FsRequest::WriteChunkDeflated {
                    path, offset, ..
                }) => {
                    format!("deflated {} @{offset}", path.as_str())
                }
                ClientRequest::Filesystem(FsRequest::HashPackage { prefix }) => {
                    format!("hash {}", prefix.as_str())
                }
                ClientRequest::Filesystem(FsRequest::Read { path }) => {
                    format!("read {}", path.as_str())
                }
                ClientRequest::LoadProject { path } => format!("load {path}"),
                other => format!("{other:?}"),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reported_path_names_the_storage_dir_a_push_replaces() {
        assert_eq!(storage_id_of("/projects/demo").as_deref(), Some("demo"));
        assert_eq!(storage_id_of("projects/demo/").as_deref(), Some("demo"));
        assert_eq!(
            storage_id_of("/projects/2026-08-30-porch/sub").as_deref(),
            Some("2026-08-30-porch")
        );
        // Not a project dir: a push must not replace it.
        assert_eq!(storage_id_of("/somewhere/else"), None);
        assert_eq!(storage_id_of("/projects/"), None);
    }
}
