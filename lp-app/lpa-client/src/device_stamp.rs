//! The board-manifest stamp: a journaled write, in chunks a board can decode
//! beside a running project.
//!
//! ⚠️ The stamp runs at the board's tightest moment by construction: the
//! flash ladder starts it after the post-flash hello, and the hello comes
//! after auto-load. A classic ESP32 with its auto-loaded project and that
//! project's compiled shader resident had 15 KB free, 7.6 KB of it in the
//! largest hole (bench, 2026-09-04), and one `FsRequest::Write` carrying
//! the 6,123-byte DOM-Z-102 manifest ran out of heap in the request
//! DECODE — the line `String` holding the frame, serde_json's unescape
//! scratch, the decoded `String` and `deserialize_smart`'s base64 attempt
//! are about four copies of the payload, in one request — and the board
//! soft-reset before any write ran
//! (`docs/defects/2026-09-04-classic-ooms-decoding-the-manifest-write.md`).
//!
//! So the manifest travels as a run of `FsRequest::WriteChunk`s of
//! [`MANIFEST_CHUNK_BYTES`] each — the wire the push already uses — and the
//! project keeps running. The alternatives, and why not, are in
//! `docs/adr/2026-09-04-the-manifest-stamp-streams-beside-a-running-project.md`:
//! stopping the project frees the heap but leaves the board dark until a
//! reboot, and a reboot re-enumerates a native-USB port after the flash
//! activity has already settled; minifying the manifest takes a third off
//! a payload whose decode SHAPE is the problem.
//!
//! ⚠️ A chunked write is a prefix at every chunk boundary (chunk 0
//! truncates), and a stamp that stops between chunks — a tab closed, a
//! cable pulled, a board reset — used to leave `/hardware.json` torn, so
//! the board lost its stamped manifest and booted on the compiled-in one
//! (`docs/defects/2026-10-02-a-closed-tab-mid-stamp-leaves-hardware-json-truncated.md`).
//! [`stamp_board_manifest`] is therefore journaled, with no new wire
//! request: the whole manifest is written to
//! [`lpc_hardware::HARDWARE_MANIFEST_NEXT_PATH`] first, then to
//! [`lpc_hardware::HARDWARE_MANIFEST_PATH`], then the staged copy is
//! deleted. The firmware's loader finishes or drops a staged copy at boot
//! (`fw-esp32-common`'s `manifest_loader`), which is exactly when a manifest
//! takes effect — so wherever the stamp stops, the board boots on a whole
//! manifest: the previous one, or the new one.

use lpc_hardware::{HARDWARE_MANIFEST_NEXT_PATH, HARDWARE_MANIFEST_PATH};
use lpc_model::{AsLpPath, LpPath};

use crate::client::{ClientOutcome, LpClient};
use crate::client_error::{ClientError, ClientResult};
use crate::client_io::ClientIo;
use crate::device_push::PushProgress;

/// Raw bytes per chunk of the manifest stamp.
///
/// Sized for the decode, not the wire: the push's
/// `lpc_wire::budget::FILE_SYNC_CHUNK_BYTES` (4 KiB) runs after a
/// `StopAllProjects` on a freed heap, while the stamp runs beside whatever
/// the board auto-loaded. Four copies of 1 KiB — escaped JSON text is near
/// its raw size — is ~5 KB of transient, in allocations of ~2 KB at most,
/// inside the 7.6 KB largest hole the bench classic had. 4 KiB would have
/// asked for the same 8–10 KB scratch that failed.
pub const MANIFEST_CHUNK_BYTES: usize = 1024;

/// Stamp `manifest` onto the board as its `/hardware.json`, journaled.
///
/// Three steps, each an ordinary file request: the whole manifest to the
/// staged path, the whole manifest to the live path, then the staged copy
/// deleted. Where an interruption lands decides what the board boots on,
/// and every case is a whole manifest:
///
/// | stopped during | live file | staged copy | the board boots on |
/// |---|---|---|---|
/// | staging | the previous stamp, untouched | a prefix | the previous stamp (the loader drops the prefix) |
/// | the live write | a prefix | whole | the new manifest (the loader finishes the stamp) |
/// | the delete | whole | whole | the new manifest |
///
/// The error says which of the first two the board is in, because they
/// differ in what the board will run.
pub async fn stamp_board_manifest<Io: ClientIo>(
    client: &mut LpClient<Io>,
    manifest: &[u8],
    chunk_bytes: usize,
    progress: PushProgress<'_>,
) -> ClientResult<ClientOutcome<()>> {
    let live = HARDWARE_MANIFEST_PATH.as_path();
    let staged = HARDWARE_MANIFEST_NEXT_PATH.as_path();
    let mut events = Vec::new();

    match write_file_in_chunks(client, staged, manifest, chunk_bytes, progress).await {
        Ok(outcome) => events.extend(outcome.events),
        Err(error) => {
            return Err(ClientError::Server(format!(
                "{error} — {HARDWARE_MANIFEST_PATH} was not touched, so the board keeps the \
                 manifest it had"
            )));
        }
    }
    match write_file_in_chunks(client, live, manifest, chunk_bytes, progress).await {
        Ok(outcome) => events.extend(outcome.events),
        Err(error) => {
            return Err(ClientError::Server(format!(
                "{error} — the whole manifest is staged at {HARDWARE_MANIFEST_NEXT_PATH}, and \
                 the board finishes the stamp when it next boots"
            )));
        }
    }
    match client.fs_delete_file(staged).await {
        Ok(outcome) => events.extend(outcome.events),
        // The manifest is written; a staged copy left behind is the same
        // bytes, and the board clears it at boot. Not a failed stamp — but
        // said, not swallowed.
        Err(error) => progress(
            format!(
                "Left {HARDWARE_MANIFEST_NEXT_PATH} for the board to clear at its next boot \
                 ({error})"
            ),
            None,
        ),
    }
    Ok(ClientOutcome::new((), events))
}

/// Write `bytes` to `path`, as one `Write` when it fits a chunk and as a
/// run of offset `WriteChunk`s otherwise.
///
/// Not atomic: a run of chunks that stops part-way leaves a prefix, which
/// this removes (best effort) and reports. A file a reader must never see
/// torn needs a journal around it — see [`stamp_board_manifest`].
///
/// Progress is label-only on purpose (`Writing /hardware.json (3/6)`): the
/// stamp's earlier steps carry no percent, and a bar that restarts at 0
/// for the second half of a flash reads as the flash going backwards.
pub async fn write_file_in_chunks<Io: ClientIo>(
    client: &mut LpClient<Io>,
    path: &LpPath,
    bytes: &[u8],
    chunk_bytes: usize,
    progress: PushProgress<'_>,
) -> ClientResult<ClientOutcome<()>> {
    let chunk_bytes = chunk_bytes.max(1);
    let display = path.as_str();
    if bytes.len() <= chunk_bytes {
        progress(format!("Writing {display}"), None);
        return client.fs_write(path, bytes.to_vec()).await;
    }
    let total = bytes.len().div_ceil(chunk_bytes);
    let mut events = Vec::new();
    for (index, chunk) in bytes.chunks(chunk_bytes).enumerate() {
        let number = index + 1;
        progress(format!("Writing {display} ({number}/{total})"), None);
        let offset = u32::try_from(index * chunk_bytes).map_err(|_| {
            ClientError::Protocol(format!("{display} is too large for a chunked write"))
        })?;
        match client.fs_write_chunk(path, offset, chunk.to_vec()).await {
            Ok(outcome) => events.extend(outcome.events),
            Err(error) => {
                return Err(abandon_partial_file(client, path, number, total, error).await);
            }
        }
    }
    Ok(ClientOutcome::new((), events))
}

/// A chunk failed: take the prefix off the board (best effort) and say
/// which state the board is in — "removed" and "could not remove" are
/// different facts, and the card should carry the true one.
async fn abandon_partial_file<Io: ClientIo>(
    client: &mut LpClient<Io>,
    path: &LpPath,
    number: usize,
    total: usize,
    error: ClientError,
) -> ClientError {
    let display = path.as_str();
    let what = format!("chunk {number}/{total} of {display} failed: {error}");
    match client.fs_delete_file(path).await {
        Ok(_) => ClientError::Server(format!("{what}; the partial file was removed")),
        Err(delete_error) => ClientError::Server(format!(
            "{what}; the partial file could not be removed ({delete_error})"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use lpc_model::AsLpPathBuf;
    use lpc_wire::{ClientRequest, FsRequest, FsResponse, WireServerMessage, WireServerMsgBody};

    use crate::scripted_io::ScriptedIo;

    const PATH: &str = "/hardware.json";

    fn manifest(len: usize) -> Vec<u8> {
        // Text, like the real manifest: `serialize_smart` sends UTF-8 as a
        // JSON string, so the bytes on the wire are the bytes in the file.
        (0..len).map(|i| b'a' + (i % 26) as u8).collect()
    }

    fn write_ok(id: u64) -> WireServerMessage {
        WireServerMessage::new(
            id,
            WireServerMsgBody::Filesystem(FsResponse::Write {
                path: PATH.as_path_buf(),
                error: None,
            }),
        )
    }

    fn chunk_response(id: u64, offset: u32, error: Option<&str>) -> WireServerMessage {
        WireServerMessage::new(
            id,
            WireServerMsgBody::Filesystem(FsResponse::WriteChunk {
                path: PATH.as_path_buf(),
                offset,
                written: 0,
                error: error.map(str::to_string),
            }),
        )
    }

    fn delete_response(id: u64, error: Option<&str>) -> WireServerMessage {
        WireServerMessage::new(
            id,
            WireServerMsgBody::Filesystem(FsResponse::DeleteFile {
                path: PATH.as_path_buf(),
                error: error.map(str::to_string),
            }),
        )
    }

    /// A manifest that fits one chunk goes the way it always did: one
    /// plain `Write`, so a small manifest on a roomy board costs one round
    /// trip and nothing about it changes.
    #[tokio::test]
    async fn a_manifest_that_fits_one_chunk_goes_as_one_write() {
        let io = ScriptedIo::new([write_ok(1)]);
        let mut client = LpClient::new(io);
        let mut labels: Vec<String> = Vec::new();
        let mut progress = |label: String, _percent: Option<u8>| labels.push(label);
        let bytes = manifest(800);

        write_file_in_chunks(
            &mut client,
            PATH.as_path_buf().as_path(),
            &bytes,
            MANIFEST_CHUNK_BYTES,
            &mut progress,
        )
        .await
        .expect("written");

        let sent = client.into_io().sent;
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert!(
            matches!(
                &sent[0].msg,
                ClientRequest::Filesystem(FsRequest::Write { data, .. }) if *data == bytes
            ),
            "{:?}",
            sent[0].msg
        );
        assert_eq!(labels, vec!["Writing /hardware.json".to_string()]);
    }

    /// The DOM-Z-102 shape: a manifest several chunks long streams as offset
    /// chunks — chunk 0 truncates, each later offset is the bytes before it —
    /// and the bytes, reassembled, are the manifest. No `Write` frame ever
    /// carries the whole file.
    #[tokio::test]
    async fn a_manifest_larger_than_a_chunk_streams_as_offset_chunks() {
        let bytes = manifest(2_500);
        let io = ScriptedIo::new([
            chunk_response(1, 0, None),
            chunk_response(2, 1_024, None),
            chunk_response(3, 2_048, None),
        ]);
        let mut client = LpClient::new(io);
        let mut labels: Vec<String> = Vec::new();
        let mut progress = |label: String, percent: Option<u8>| {
            assert_eq!(percent, None, "label-only: the flash bar must not restart");
            labels.push(label);
        };

        write_file_in_chunks(
            &mut client,
            PATH.as_path_buf().as_path(),
            &bytes,
            1_024,
            &mut progress,
        )
        .await
        .expect("written");

        let sent = client.into_io().sent;
        let mut reassembled = Vec::new();
        let mut offsets = Vec::new();
        for message in &sent {
            match &message.msg {
                ClientRequest::Filesystem(FsRequest::WriteChunk { offset, data, path }) => {
                    assert_eq!(path.as_str(), PATH);
                    assert_eq!(
                        *offset as usize,
                        reassembled.len(),
                        "each offset is the length so far"
                    );
                    assert!(data.len() <= 1_024, "{} bytes in one chunk", data.len());
                    offsets.push(*offset);
                    reassembled.extend_from_slice(data);
                }
                other => panic!("a chunked write sends only chunks: {other:?}"),
            }
        }
        assert_eq!(offsets, vec![0, 1_024, 2_048]);
        assert_eq!(reassembled, bytes, "the board ends up with the manifest");
        assert_eq!(
            labels,
            vec![
                "Writing /hardware.json (1/3)".to_string(),
                "Writing /hardware.json (2/3)".to_string(),
                "Writing /hardware.json (3/3)".to_string(),
            ]
        );
    }

    /// A chunk the board refuses leaves a prefix on the board. The
    /// conversation takes it off and says so — and names the chunk, so the
    /// card's terminal shows where the write stopped.
    #[tokio::test]
    async fn a_refused_chunk_removes_the_partial_file_and_says_so() {
        let bytes = manifest(2_500);
        let io = ScriptedIo::new([
            chunk_response(1, 0, None),
            chunk_response(
                2,
                1_024,
                Some("offset mismatch: file is 0 bytes, chunk at 1024"),
            ),
            delete_response(3, None),
        ]);
        let mut client = LpClient::new(io);
        let mut progress = |_label: String, _percent: Option<u8>| {};

        let error = write_file_in_chunks(
            &mut client,
            PATH.as_path_buf().as_path(),
            &bytes,
            1_024,
            &mut progress,
        )
        .await
        .expect_err("refused");

        let message = error.to_string();
        assert!(message.contains("chunk 2/3"), "{message}");
        assert!(message.contains("offset mismatch"), "{message}");
        assert!(message.contains("partial file was removed"), "{message}");
        let sent = client.into_io().sent;
        assert_eq!(sent.len(), 3, "two chunks, then the delete: {sent:?}");
        assert!(
            matches!(
                &sent[2].msg,
                ClientRequest::Filesystem(FsRequest::DeleteFile { path }) if path.as_str() == PATH
            ),
            "{:?}",
            sent[2].msg
        );
    }

    /// The bench's own shape: the board stops answering mid-write (an OOM
    /// reset, say) and the clean-up cannot be confirmed either. The error
    /// says the prefix could not be removed.
    #[tokio::test]
    async fn a_board_that_stops_answering_mid_way_is_reported_with_the_partial_file_state() {
        let bytes = manifest(2_500);
        let io = ScriptedIo::new([chunk_response(1, 0, None)]);
        let mut client = LpClient::new(io);
        let mut progress = |_label: String, _percent: Option<u8>| {};

        let error = write_file_in_chunks(
            &mut client,
            PATH.as_path_buf().as_path(),
            &bytes,
            1_024,
            &mut progress,
        )
        .await
        .expect_err("the board went away");

        let message = error.to_string();
        assert!(message.contains("chunk 2/3"), "{message}");
        assert!(message.contains("could not be removed"), "{message}");
    }

    /// The stamp's order is the whole fix: every byte of the manifest
    /// reaches the staged path before the first request names the live
    /// one, and the staged copy is deleted last.
    #[tokio::test]
    async fn a_stamp_stages_the_whole_manifest_before_it_touches_hardware_json() {
        let bytes = manifest(2_500);
        let io = ScriptedIo::new([
            chunk_response(1, 0, None),
            chunk_response(2, 1_024, None),
            chunk_response(3, 2_048, None),
            chunk_response(4, 0, None),
            chunk_response(5, 1_024, None),
            chunk_response(6, 2_048, None),
            delete_response(7, None),
        ]);
        let mut client = LpClient::new(io);
        let mut labels: Vec<String> = Vec::new();
        let mut progress = |label: String, _percent: Option<u8>| labels.push(label);

        stamp_board_manifest(&mut client, &bytes, 1_024, &mut progress)
            .await
            .expect("stamped");

        let sent = client.into_io().sent;
        let steps: Vec<(String, &str)> = sent
            .iter()
            .map(|message| match &message.msg {
                ClientRequest::Filesystem(FsRequest::WriteChunk { path, .. }) => {
                    (path.as_str().to_string(), "chunk")
                }
                ClientRequest::Filesystem(FsRequest::DeleteFile { path }) => {
                    (path.as_str().to_string(), "delete")
                }
                other => panic!("a stamp sends chunks and one delete: {other:?}"),
            })
            .collect();
        let next = HARDWARE_MANIFEST_NEXT_PATH.to_string();
        let live = HARDWARE_MANIFEST_PATH.to_string();
        assert_eq!(
            steps,
            vec![
                (next.clone(), "chunk"),
                (next.clone(), "chunk"),
                (next.clone(), "chunk"),
                (live.clone(), "chunk"),
                (live.clone(), "chunk"),
                (live, "chunk"),
                (next, "delete"),
            ]
        );
        assert_eq!(labels.len(), 6, "{labels:?}");
    }

    /// Cut while staging: the live manifest was never named, and the error
    /// says the board keeps the one it had — the claim the card carries.
    #[tokio::test]
    async fn a_stamp_cut_while_staging_never_names_hardware_json_and_says_the_board_keeps_its_manifest()
     {
        let bytes = manifest(2_500);
        let io = ScriptedIo::new([chunk_response(1, 0, None)]);
        let mut client = LpClient::new(io);
        let mut progress = |_label: String, _percent: Option<u8>| {};

        let error = stamp_board_manifest(&mut client, &bytes, 1_024, &mut progress)
            .await
            .expect_err("the board went away");

        let message = error.to_string();
        assert!(message.contains("keeps the manifest it had"), "{message}");
        let sent = client.into_io().sent;
        for message in &sent {
            let path = match &message.msg {
                ClientRequest::Filesystem(FsRequest::WriteChunk { path, .. })
                | ClientRequest::Filesystem(FsRequest::Write { path, .. })
                | ClientRequest::Filesystem(FsRequest::DeleteFile { path }) => path.as_str(),
                other => panic!("{other:?}"),
            };
            assert_eq!(path, HARDWARE_MANIFEST_NEXT_PATH, "{sent:?}");
        }
    }

    /// Cut during the live write: the staged copy is whole, and the error
    /// says the board finishes the stamp at its next boot.
    #[tokio::test]
    async fn a_stamp_cut_during_the_live_write_says_the_board_finishes_it_at_boot() {
        let bytes = manifest(2_500);
        let io = ScriptedIo::new([
            chunk_response(1, 0, None),
            chunk_response(2, 1_024, None),
            chunk_response(3, 2_048, None),
            chunk_response(4, 0, None),
        ]);
        let mut client = LpClient::new(io);
        let mut progress = |_label: String, _percent: Option<u8>| {};

        let error = stamp_board_manifest(&mut client, &bytes, 1_024, &mut progress)
            .await
            .expect_err("the board went away");

        let message = error.to_string();
        assert!(message.contains("finishes the stamp"), "{message}");
        assert!(message.contains(HARDWARE_MANIFEST_NEXT_PATH), "{message}");
    }
}
