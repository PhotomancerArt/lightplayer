//! The push conversation, in one runtime-neutral place.
//!
//! Studio has had these three steps since the sim shipped — they are what
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
//! 2. StopAll → clear → chunked writes → LoadProject   (project_deploy's
//!    order, file_sync_ops' chunking — unchanged, on purpose)
//! 3. HashPackage         → does the board hold exactly the library's bytes?
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
//! Step 2 never writes over the dir the board runs from. It writes the
//! OTHER slot ([`other_slot`]: `demo` ↔ `demo-b`) and removes the old dir
//! only once the new one has loaded and hashed. A board that refuses the new
//! project (format, parse, heap, a recovery gate) is told to load the old
//! dir again, which is still whole — and which `startup_project` still
//! names, since the server persists that only on a load that succeeded. A
//! refused push used to leave the board dark over a folder it had refused.
//!
//! Step 3 is not optional. A serial wire drops bytes; a truncated write that
//! loaded anyway would leave a board running something no library has, and
//! the next sync verdict would be computed against a lie.

use crate::client::LpClient;
use crate::client_error::{ClientError, ClientResult};
use crate::client_io::ClientIo;

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
    let storage_id = match &previous {
        Some(running) => other_slot(running),
        None => fallback_storage_id.to_string(),
    };

    progress(format!("Sending the project to {storage_id}"), Some(20));
    let landed = match client.replace_and_load_project(&storage_id, files).await {
        Ok(_) => {
            progress("Checking what the board received".to_string(), Some(85));
            match client.hash_package(&storage_id).await {
                Ok(outcome) if outcome.value == expected_hash => Ok(outcome.value),
                // The bytes on the board are not the bytes in the library.
                // Saying so beats a green card over a project nobody has.
                Ok(outcome) => Err(ClientError::Protocol(format!(
                    "the board ended up with different bytes than the library sent \
                     (device {}, library {expected_hash}) — the project was not \
                     fully written",
                    outcome.value
                ))),
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    };

    match (landed, previous) {
        (Ok(hash), previous) => {
            if let Some(old) = previous.filter(|old| *old != storage_id) {
                // Best-effort: the new project runs and is verified. A dir
                // that survives this is the next push's target slot, which
                // that push clears first.
                let _ = client.delete_project_dir(&old).await;
            }
            progress("Done".to_string(), Some(100));
            Ok(PushReport { storage_id, hash })
        }
        (Err(error), Some(old)) if old != storage_id && running.is_none() => {
            // The board ran nothing before and runs nothing now. Its saved
            // folder was never touched and `startup_project` still names it
            // (the server persists only a load that succeeded), so there is
            // nothing to restore: drop what it refused and say what is left.
            let _ = client.delete_project_dir(&storage_id).await;
            Err(with_note(
                error,
                &format!(
                    "the board is still running nothing; its saved project ({old}) \
                     is untouched and is what it tries at the next boot"
                ),
            ))
        }
        (Err(error), Some(old)) if old != storage_id => {
            // The old dir was never touched: put it back on, then drop what
            // the board refused. The error is the refusal, with what became
            // of the board appended — the card must not guess.
            progress(
                "The board refused it; restoring what it ran".to_string(),
                Some(90),
            );
            let restored = client
                .project_load(&crate::project_deploy::project_load_path(&old))
                .await;
            let _ = client.delete_project_dir(&storage_id).await;
            Err(with_restore_note(error, &old, restored.is_ok()))
        }
        (Err(error), _) => Err(error),
    }
}

/// The dir a push writes when the board runs from `running`: never the one
/// it runs from, so a refusal leaves that one whole. Two slots alternate
/// (`demo` ↔ `demo-b`), so a board never holds more than two copies and the
/// dir name stays recognisable.
pub(crate) fn other_slot(running: &str) -> String {
    match running.strip_suffix(SLOT_B_SUFFIX) {
        Some(base) if !base.is_empty() => base.to_string(),
        _ => format!("{running}{SLOT_B_SUFFIX}"),
    }
}

const SLOT_B_SUFFIX: &str = "-b";

fn with_restore_note(error: ClientError, old: &str, restored: bool) -> ClientError {
    let note = match restored {
        true => format!("the board is running its previous project ({old}) again"),
        false => format!(
            "the board could not reload its previous project ({old}) either; \
             it is still on the board and loads at the next boot"
        ),
    };
    with_note(error, &note)
}

fn with_note(error: ClientError, note: &str) -> ClientError {
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
async fn saved_startup_project<Io: ClientIo>(client: &mut LpClient<Io>) -> Option<String> {
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

    #[test]
    fn a_push_writes_the_slot_the_board_does_not_run_from() {
        assert_eq!(other_slot("demo"), "demo-b");
        assert_eq!(other_slot("demo-b"), "demo");
        assert_eq!(other_slot("2026-08-30-porch"), "2026-08-30-porch-b");
        // A dir literally named "-b" has no base; it alternates with "-b-b".
        assert_eq!(other_slot("-b"), "-b-b");
    }
}
