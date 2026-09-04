//! Two short conversations a device coarse-effect needs, in one
//! runtime-neutral place: *is the board ready to be written to*, and *take
//! the project off it*.
//!
//! Both live here for the same reason [`device_push`](crate::device_push)
//! does — they run inside an effect that borrowed one serial port, over
//! whatever [`ClientIo`] that platform has, with no session, no log sink and
//! no pull loop underneath them.

use crate::client::LpClient;
use crate::client_error::{ClientError, ClientResult};
use crate::client_io::ClientIo;

/// How many times [`wait_until_ready`] asks before giving up.
///
/// A count rather than a duration on purpose: this code has no clock, and
/// the caller's io already bounds each attempt (five seconds, in the browser
/// port io). Five attempts is therefore about 25 s of patience, sized to fit
/// inside the stamp's own 30 s activity deadline with the write still to do.
///
/// ⚠️ The bench's slowest observation was ~40 s (G1, 2026-08-31), so a board
/// at the far end of that range still outlasts this wait. That degrades
/// honestly — the flash stands and the card says the manifest write was
/// not confirmed — but if it turns out to be common, this constant and
/// `RosterConfig::stamp_deadline_ms` move together or not at all.
pub const READY_ATTEMPTS: u32 = 5;

/// Wait for a board to answer anything at all before writing to it.
///
/// The lesson the old provisioning wizard learned and round 2 had to learn
/// again (G1 bench, 2026-08-31): a just-flashed board formats its littlefs
/// on first boot, and while it does, fs writes are simply not answered. The
/// stamp that followed a flash straight in burned ~40 s of retried writes
/// and then reported failure at a board that was about to be perfectly fine.
///
/// So: ask something CHEAP and idempotent first —
/// `ListLoadedProjects` reads no files and changes nothing — and retry it
/// under a budget of its own. Each attempt is bounded by the caller's io;
/// [`READY_ATTEMPTS`] bounds how many there are. A late answer to an
/// abandoned attempt is dropped by the client's own request correlation, so
/// retrying cannot desynchronize the wire.
pub async fn wait_until_ready<Io: ClientIo>(
    client: &mut LpClient<Io>,
    attempts: u32,
    progress: crate::device_push::PushProgress<'_>,
) -> ClientResult<()> {
    let mut last: Option<ClientError> = None;
    for attempt in 1..=attempts.max(1) {
        match client.project_list_loaded().await {
            Ok(_) => return Ok(()),
            Err(error) => {
                progress(
                    format!("Waiting for the board to come back ({attempt}/{attempts})"),
                    None,
                );
                last = Some(error);
            }
        }
    }
    Err(last.unwrap_or_else(|| ClientError::Protocol("the board never answered".to_string())))
}

/// What a finished removal did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoveReport {
    /// The project storage dir under `/projects/` that was deleted.
    pub storage_id: String,
    /// Whether the board had actually reported running it. `false` means the
    /// fallback slot was cleared instead — honest, and worth saying.
    pub was_loaded: bool,
}

/// Take the loaded project off a device that is already listening.
///
/// ```text
/// 1. ListLoadedProjects  → which storage dir does this board run from?
/// 2. StopAllProjects     → nothing is executing out of the dir being deleted
/// 3. FsRequest::DeleteDir → the dir itself
/// ```
///
/// Step 1 is the whole point: the model never names a path. A board flashed
/// by the CLI, or reflashed with its littlefs intact, runs from a dir of its
/// own — `/projects/zook-dome` on the bench's V3 — and deleting a guessed
/// `demo` beside it would leave the board running the very project the user
/// asked to remove.
///
/// Step 2 is not optional either. Deleting the files under a running project
/// is how a board ends up executing half of something.
///
/// `fallback_storage_id` is used only when the board reports nothing loaded,
/// which is the race between the card offering the verb and the effect
/// running. Nothing is created; a delete of an absent dir is a no-op.
pub async fn remove_project<Io: ClientIo>(
    client: &mut LpClient<Io>,
    fallback_storage_id: &str,
    progress: crate::device_push::PushProgress<'_>,
) -> ClientResult<RemoveReport> {
    progress("Asking the board what it is running".to_string(), Some(10));
    let loaded = client.project_list_loaded().await?;
    let reported = loaded
        .value
        .first()
        .and_then(|project| crate::device_push::storage_id_of(project.path.as_str()));
    let was_loaded = reported.is_some();
    let storage_id = reported.unwrap_or_else(|| fallback_storage_id.to_string());

    progress("Stopping the project".to_string(), Some(40));
    client.stop_all_projects().await?;

    progress(format!("Deleting {storage_id}"), Some(70));
    client.delete_project_dir(&storage_id).await?;

    progress("Done".to_string(), Some(100));
    Ok(RemoveReport {
        storage_id,
        was_loaded,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use lpc_wire::{WireServerMessage, WireServerMsgBody};

    use crate::scripted_io::ScriptedIo;

    /// The walk: ask, stop, delete — and the dir deleted is the one the
    /// BOARD named, never a guess.
    #[tokio::test]
    async fn a_removal_deletes_the_dir_the_board_reported_running() {
        let io = ScriptedIo::new([
            loaded_response(1, Some("/projects/zook-dome")),
            WireServerMessage::new(2, WireServerMsgBody::StopAllProjects),
            delete_dir_response(3, None),
        ]);
        let mut client = LpClient::new(io);
        let mut noted: Vec<String> = Vec::new();
        let mut progress = |label: String, _percent: Option<u8>| noted.push(label);

        let report = remove_project(&mut client, "demo", &mut progress)
            .await
            .expect("removed");

        assert_eq!(report.storage_id, "zook-dome");
        assert!(report.was_loaded);
        let sent = client.into_io().sent;
        assert!(
            matches!(sent[1].msg, lpc_wire::ClientRequest::StopAllProjects),
            "the project stops before its files go: {:?}",
            sent[1].msg
        );
        assert!(
            format!("{:?}", sent[2].msg).contains("zook-dome"),
            "the delete names the board's own dir: {:?}",
            sent[2].msg
        );
    }

    /// The race between the card offering the verb and the effect running:
    /// the board now reports nothing. The fallback slot is cleared, and the
    /// report says the board was not actually running it.
    #[tokio::test]
    async fn a_board_reporting_nothing_falls_back_without_claiming_it_was_loaded() {
        let io = ScriptedIo::new([
            loaded_response(1, None),
            WireServerMessage::new(2, WireServerMsgBody::StopAllProjects),
            delete_dir_response(3, None),
        ]);
        let mut client = LpClient::new(io);
        let mut progress = |_label: String, _percent: Option<u8>| {};

        let report = remove_project(&mut client, "demo", &mut progress)
            .await
            .expect("removed");

        assert_eq!(report.storage_id, "demo");
        assert!(!report.was_loaded);
    }

    /// A board still formatting its flash refuses the first asks and then
    /// answers. The wait absorbs that instead of the write hitting it.
    #[tokio::test]
    async fn the_ready_wait_retries_a_board_that_is_not_answering_yet() {
        // Two dropped attempts (the io reports the wire lost), then a real
        // answer to the third.
        let io = ScriptedIo::new([loaded_response(3, None)]).with_drops(2);
        let mut client = LpClient::new(io);
        let mut noted: Vec<String> = Vec::new();
        let mut progress = |label: String, _percent: Option<u8>| noted.push(label);

        wait_until_ready(&mut client, READY_ATTEMPTS, &mut progress)
            .await
            .expect("the board came back");

        assert_eq!(noted.len(), 2, "each wait says so once: {noted:?}");
        assert!(noted[0].contains("Waiting for the board"), "{noted:?}");
    }

    /// A board that never answers is a failure with its own message — never
    /// a silent write into a device that is not listening.
    #[tokio::test]
    async fn the_ready_wait_gives_up_after_its_budget() {
        let io = ScriptedIo::new([]).with_drops(u32::MAX);
        let mut client = LpClient::new(io);
        let mut progress = |_label: String, _percent: Option<u8>| {};

        let error = wait_until_ready(&mut client, 3, &mut progress)
            .await
            .expect_err("refused");

        assert!(!error.to_string().is_empty(), "{error:?}");
    }

    fn loaded_response(id: u64, path: Option<&str>) -> WireServerMessage {
        use lpc_model::AsLpPathBuf;
        let projects = path
            .map(|path| {
                lpc_wire::LoadedProject::new(lpc_wire::WireProjectHandle(1), path.as_path_buf())
            })
            .into_iter()
            .collect();
        WireServerMessage::new(id, WireServerMsgBody::ListLoadedProjects { projects })
    }

    fn delete_dir_response(id: u64, error: Option<String>) -> WireServerMessage {
        use lpc_model::AsLpPathBuf;
        WireServerMessage::new(
            id,
            WireServerMsgBody::Filesystem(lpc_wire::FsResponse::DeleteDir {
                path: "/projects/zook-dome".as_path_buf(),
                error,
            }),
        )
    }
}
