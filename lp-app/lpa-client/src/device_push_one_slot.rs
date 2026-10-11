//! The one-slot push: what a board whose filesystem has transactions (the
//! tree store) gets — the project replaced in its own folder, in one batch.
//!
//! ```text
//! (push_project: ListLoaded → StopAll → BeginBatch = atomic)
//! DeleteDir → writes (deflated) → LoadProject → HashPackage → CommitBatch
//! ```
//!
//! The commit comes last: the board commits only a project it has loaded
//! and holds byte for byte. Anything before it — an error, a refusal (the
//! board will not load the new project), a hash that does not match, a
//! timeout — aborts the batch, and the old project is whole in the same
//! folder, so the push loads it again. A board that resets mid-load comes
//! back on the old committed state with nothing to do. No `-b` slot, no
//! cleanup of an old one, and no "remove the old project to make room" on
//! the common path.
//!
//! Inside one batch the old project's records stay reachable until the
//! commit, so a project that does not fit beside the old one is still out
//! of room in the batch. Then (and only then) the old project goes in a
//! batch of its own, committed, and the new one is written in a second.

use crate::client::LpClient;
use crate::client_error::{ClientError, ClientResult};
use crate::client_io::ClientIo;
use crate::device_push::{
    PushProgress, PushReport, is_no_space, verify_hash, with_note, with_restore_note,
};
use crate::push_files::{
    BatchUse, DeployPlan, LpClientSink, abort_batch, begin_batch, clear_project_dir, commit_batch,
};

/// The one-slot conversation, after `push_project` has stopped the board
/// and its `BeginBatch` was answered `atomic: true` (the batch is open).
/// `running` is what the board ran, `previous` what it ran or boots.
pub(crate) async fn push_one_slot<Io: ClientIo>(
    client: &mut LpClient<Io>,
    files: &[(String, Vec<u8>)],
    expected_hash: &str,
    fallback_storage_id: &str,
    running: Option<String>,
    previous: Option<String>,
    progress: PushProgress<'_>,
) -> ClientResult<PushReport> {
    let storage_id = previous
        .clone()
        .unwrap_or_else(|| fallback_storage_id.to_string());
    progress("Sending the project to the board".to_string(), Some(20));
    let landed = replace_in_batch(client, &storage_id, files, expected_hash, progress).await;
    let error = match landed {
        Ok(hash) => {
            progress("Done".to_string(), Some(100));
            return Ok(PushReport { storage_id, hash });
        }
        Err(error) => error,
    };
    // Whatever went wrong, the board holds the batch open or has dropped
    // it: drop it (best effort — the request that failed may never have
    // arrived), and the old project is whole.
    let _ = abort_batch(&mut LpClientSink::new(client)).await;
    let Some(old) = previous else {
        return Err(error);
    };
    if running.is_none() {
        // The board ran nothing before and runs nothing now; its saved
        // project is as it was, and is what it tries at the next boot.
        return Err(with_note(
            error,
            &format!(
                "the board is still running nothing; its saved project ({old}) \
                 is untouched and is what it tries at the next boot"
            ),
        ));
    }
    if is_no_space(&error) {
        return make_room_and_push(client, files, expected_hash, &old, progress).await;
    }
    progress(
        "The board refused it; restoring what it ran".to_string(),
        Some(90),
    );
    let restored = client
        .project_load(&crate::project_deploy::project_load_path(&old))
        .await;
    Err(with_restore_note(error, &old, restored.is_ok()))
}

/// Inside the open batch: replace `storage_id` with `files`, load it, check
/// its hash, commit.
async fn replace_in_batch<Io: ClientIo>(
    client: &mut LpClient<Io>,
    storage_id: &str,
    files: &[(String, Vec<u8>)],
    expected_hash: &str,
    progress: PushProgress<'_>,
) -> ClientResult<String> {
    let plan = DeployPlan {
        stop: false,
        batch: BatchUse::None,
        clear: true,
        deflate: true,
        load: true,
        expected_hash: None,
    };
    client
        .deploy_slice(storage_id, files, plan, &mut |_| {})
        .await?;
    progress("Checking what the board received".to_string(), Some(85));
    let hash = verify_hash(client, storage_id, expected_hash).await?;
    commit_batch(&mut LpClientSink::new(client)).await?;
    Ok(hash)
}

/// The new project does not fit beside the old one: remove the old one in a
/// batch of its own (committed: its records are free only once a root
/// without them is written), then push into the same folder again.
async fn make_room_and_push<Io: ClientIo>(
    client: &mut LpClient<Io>,
    files: &[(String, Vec<u8>)],
    expected_hash: &str,
    old: &str,
    progress: PushProgress<'_>,
) -> ClientResult<PushReport> {
    progress(
        format!("This board can't hold the new project beside {old} — removing it to make room"),
        Some(40),
    );
    let removed = {
        let mut sink = LpClientSink::new(client);
        match begin_batch(&mut sink).await {
            Ok(_) => match clear_project_dir(&mut sink, old).await {
                Ok(()) => commit_batch(&mut sink).await,
                Err(error) => Err(error),
            },
            Err(error) => Err(error),
        }
    };
    if let Err(error) = removed {
        let _ = abort_batch(&mut LpClientSink::new(client)).await;
        let restored = client
            .project_load(&crate::project_deploy::project_load_path(old))
            .await;
        return Err(with_restore_note(error, old, restored.is_ok()));
    }
    progress(format!("Sending the project to {old}"), Some(50));
    let landed = match begin_batch(&mut LpClientSink::new(client)).await {
        Ok(_) => replace_in_batch(client, old, files, expected_hash, progress).await,
        Err(error) => Err(error),
    };
    match landed {
        Ok(hash) => {
            progress("Done".to_string(), Some(100));
            Ok(PushReport {
                storage_id: old.to_string(),
                hash,
            })
        }
        Err(error) => {
            let _ = abort_batch(&mut LpClientSink::new(client)).await;
            Err(match is_no_space(&error) {
                true => ClientError::Server(format!(
                    "this board can't hold the new project beside the old one, so the old \
                     copy ({old}) was removed to make room — and the project still doesn't \
                     fit on the board"
                )),
                false => with_note(
                    error,
                    &format!(
                        "the previous project ({old}) was removed to make room, so the \
                         board is running nothing"
                    ),
                ),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use lpc_wire::BatchOp;

    use crate::client::LpClient;
    use crate::device_push::push_project;
    use crate::device_push::test_script::*;
    use crate::scripted_io::ScriptedIo;

    /// The common path: one folder, one batch, committed after the load and
    /// the hash. No `-b` slot, no old folder to remove.
    #[tokio::test]
    async fn a_push_replaces_the_project_in_its_own_folder_in_one_batch() {
        let io = ScriptedIo::new([
            loaded(1, "demo"),
            stopped(2),
            batch(3, BatchOp::Begin, true),
            deleted(4),
            written(5, None),
            written(6, None),
            loaded_ok(7),
            hashed(8, HASH),
            batch(9, BatchOp::Commit, true),
        ]);
        let mut client = LpClient::new(io);
        let mut noted: Vec<String> = Vec::new();
        let mut progress = |label: String, _percent: Option<u8>| noted.push(label);

        let report = push_project(&mut client, &files(), HASH, "fallback", &mut progress)
            .await
            .expect("pushed");

        assert_eq!(report.storage_id, "demo", "the same folder");
        assert_eq!(
            ops(&client.into_io().sent),
            [
                "list-loaded",
                "stop",
                "begin",
                "delete demo",
                "write /projects/demo/project.json",
                "write /projects/demo/main.glsl",
                "load projects/demo",
                "hash /projects/demo",
                "commit",
            ]
        );
        assert!(!noted.iter().any(|label| label.contains("-b")), "{noted:?}");
    }

    /// The board refuses the new project at its load: the batch is dropped
    /// (the old project is whole again) and the old project is loaded.
    #[tokio::test]
    async fn a_refused_load_aborts_and_reloads_the_old_project() {
        let io = ScriptedIo::new([
            loaded(1, "demo"),
            stopped(2),
            batch(3, BatchOp::Begin, true),
            deleted(4),
            written(5, None),
            written(6, None),
            server_error(7, "project format 99 is newer than this board"),
            batch(8, BatchOp::Abort, true),
            loaded_ok(9),
        ]);
        let mut client = LpClient::new(io);
        let mut progress = |_label: String, _percent: Option<u8>| {};

        let error = push_project(&mut client, &files(), HASH, "fallback", &mut progress)
            .await
            .expect_err("refused");

        assert!(
            error
                .to_string()
                .contains("the board is running its previous project (demo) again"),
            "{error}"
        );
        assert_eq!(
            ops(&client.into_io().sent),
            [
                "list-loaded",
                "stop",
                "begin",
                "delete demo",
                "write /projects/demo/project.json",
                "write /projects/demo/main.glsl",
                "load projects/demo",
                "abort",
                "load projects/demo",
            ]
        );
    }

    /// The board holds other bytes than the library sent: never committed.
    #[tokio::test]
    async fn a_hash_mismatch_aborts() {
        let io = ScriptedIo::new([
            loaded(1, "demo"),
            stopped(2),
            batch(3, BatchOp::Begin, true),
            deleted(4),
            written(5, None),
            written(6, None),
            loaded_ok(7),
            hashed(8, "not-the-library"),
            batch(9, BatchOp::Abort, true),
            loaded_ok(10),
        ]);
        let mut client = LpClient::new(io);
        let mut progress = |_label: String, _percent: Option<u8>| {};

        let error = push_project(&mut client, &files(), HASH, "fallback", &mut progress)
            .await
            .expect_err("mismatch");

        assert!(error.to_string().contains("different bytes"), "{error}");
        let sent = ops(&client.into_io().sent);
        assert!(!sent.contains(&"commit".to_string()), "{sent:?}");
        assert_eq!(sent[8], "abort");
    }

    /// A write that times out (the board stopped answering) aborts, and the
    /// old project is reloaded once the board answers again.
    #[tokio::test]
    async fn a_lost_answer_aborts_without_relying_on_the_lost_request() {
        let io = ScriptedIo::new([
            loaded(1, "demo"),
            stopped(2),
            batch(3, BatchOp::Begin, true),
            deleted(4),
            // Write 5 is never answered: the script ends, the link is lost.
        ]);
        let mut client = LpClient::new(io);
        let mut progress = |_label: String, _percent: Option<u8>| {};

        let error = push_project(&mut client, &files(), HASH, "fallback", &mut progress)
            .await
            .expect_err("lost");

        let sent = ops(&client.into_io().sent);
        assert!(sent.contains(&"abort".to_string()), "{sent:?}");
        assert!(!sent.contains(&"commit".to_string()), "{sent:?}");
        let _ = error;
    }

    /// A fresh board (nothing running, nothing saved) writes the fallback
    /// folder, in one batch.
    #[tokio::test]
    async fn a_fresh_board_gets_the_fallback_folder() {
        let io = ScriptedIo::new([
            nothing_loaded(1),
            // `saved_startup_project`: no config on the board.
            read_missing(2),
            stopped(3),
            batch(4, BatchOp::Begin, true),
            deleted(5),
            written(6, None),
            written(7, None),
            loaded_ok(8),
            hashed(9, HASH),
            batch(10, BatchOp::Commit, true),
        ]);
        let mut client = LpClient::new(io);
        let mut progress = |_label: String, _percent: Option<u8>| {};

        let report = push_project(&mut client, &files(), HASH, "fallback", &mut progress)
            .await
            .expect("pushed");
        assert_eq!(report.storage_id, "fallback");
    }

    /// No room beside the old project, even in the batch: the old project
    /// goes in a batch of its own, committed, and the new one is written in
    /// a second.
    #[tokio::test]
    async fn no_room_beside_the_old_project_removes_it_in_its_own_batch_first() {
        let io = ScriptedIo::new([
            loaded(1, "demo"),
            stopped(2),
            batch(3, BatchOp::Begin, true),
            deleted(4),
            written(5, Some(NO_SPACE)),
            batch(6, BatchOp::Abort, true),
            // Batch one: the old project goes.
            batch(7, BatchOp::Begin, true),
            deleted(8),
            batch(9, BatchOp::Commit, true),
            // Batch two: the new one.
            batch(10, BatchOp::Begin, true),
            deleted(11),
            written(12, None),
            written(13, None),
            loaded_ok(14),
            hashed(15, HASH),
            batch(16, BatchOp::Commit, true),
        ]);
        let mut client = LpClient::new(io);
        let mut noted: Vec<String> = Vec::new();
        let mut progress = |label: String, _percent: Option<u8>| noted.push(label);

        let report = push_project(&mut client, &files(), HASH, "fallback", &mut progress)
            .await
            .expect("pushed after making room");

        assert_eq!(report.storage_id, "demo");
        assert_eq!(
            ops(&client.into_io().sent),
            [
                "list-loaded",
                "stop",
                "begin",
                "delete demo",
                "write /projects/demo/project.json",
                "abort",
                "begin",
                "delete demo",
                "commit",
                "begin",
                "delete demo",
                "write /projects/demo/project.json",
                "write /projects/demo/main.glsl",
                "load projects/demo",
                "hash /projects/demo",
                "commit",
            ]
        );
        assert!(
            noted.iter().any(|label| label.contains("make room")),
            "{noted:?}"
        );
    }

    /// Still no room with the old project gone: the error says the old copy
    /// went, in plain words.
    #[tokio::test]
    async fn a_project_too_big_even_alone_says_the_old_copy_was_removed() {
        let io = ScriptedIo::new([
            loaded(1, "demo"),
            stopped(2),
            batch(3, BatchOp::Begin, true),
            deleted(4),
            written(5, Some(NO_SPACE)),
            batch(6, BatchOp::Abort, true),
            batch(7, BatchOp::Begin, true),
            deleted(8),
            batch(9, BatchOp::Commit, true),
            batch(10, BatchOp::Begin, true),
            deleted(11),
            written(12, Some(NO_SPACE)),
            batch(13, BatchOp::Abort, true),
        ]);
        let mut client = LpClient::new(io);
        let mut progress = |_label: String, _percent: Option<u8>| {};

        let error = push_project(&mut client, &files(), HASH, "fallback", &mut progress)
            .await
            .expect_err("does not fit");

        let message = error.to_string();
        assert!(message.contains("removed to make room"), "{message}");
        assert!(message.contains("still doesn't fit"), "{message}");
    }
}
