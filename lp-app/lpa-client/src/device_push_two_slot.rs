//! The two-slot push: what a board without transactions (littlefs) gets.
//!
//! **Delete at adoption.** Once the tree store is the C6's filesystem, every
//! board answers `BeginBatch` with `atomic: true` and runs
//! [`crate::device_push_one_slot`]; this file, its tests and [`other_slot`]
//! go in one change then.
//!
//! The push never writes over the dir the board runs from. It writes the
//! OTHER slot ([`other_slot`]: `demo` ↔ `demo-b`) and removes the old dir
//! only once the new one has loaded and hashed. A board that refuses the new
//! project (format, parse, heap, a recovery gate) is told to load the old
//! dir again, which is still whole — and which `startup_project` still
//! names, since the server persists that only on a load that succeeded. A
//! refused push used to leave the board dark over a folder it had refused.
//!
//! Two copies need room for two copies. When the board runs out of space
//! writing the second one, the push removes the partial new slot and the old
//! one and writes again into the old folder's own name: the old project is
//! gone either way, and a power cut mid-way leaves `startup_project` naming a
//! folder the next push replaces normally. A project that still does not fit
//! alone ends in an error that says the old copy went.

use crate::client::LpClient;
use crate::client_error::{ClientError, ClientResult};
use crate::client_io::ClientIo;
use crate::device_push::{
    PushProgress, PushReport, is_no_space, verify_hash, with_note, with_restore_note,
};
use crate::push_files::{BatchUse, DeployPlan};

/// The two-slot conversation, after `push_project` has stopped the board
/// and its `BeginBatch` was answered `atomic: false`. `running` is what the
/// board ran, `previous` what it ran or boots.
pub(crate) async fn push_two_slot<Io: ClientIo>(
    client: &mut LpClient<Io>,
    files: &[(String, Vec<u8>)],
    expected_hash: &str,
    fallback_storage_id: &str,
    running: Option<String>,
    previous: Option<String>,
    progress: PushProgress<'_>,
) -> ClientResult<PushReport> {
    let storage_id = match &previous {
        Some(running) => other_slot(running),
        None => fallback_storage_id.to_string(),
    };

    progress(format!("Sending the project to {storage_id}"), Some(20));
    // Already stopped (`push_project` stopped the board before it asked
    // about batches).
    let landed = write_and_verify(client, &storage_id, files, expected_hash, false, progress).await;

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
        (Err(error), Some(old)) if old != storage_id && is_no_space(&error) => {
            // The board cannot hold two copies. Everything is already
            // stopped, so the old copy can go: drop the partial new slot,
            // then the old one, and write again into the OLD folder's own
            // name. A power cut mid-way then leaves `startup_project` naming
            // a folder the next push replaces normally — a retry into the
            // other slot would orphan a partial folder for good.
            progress(
                format!("This board can't hold two copies — removing {old} to make room"),
                Some(40),
            );
            let cleared = match client.delete_project_dir(&storage_id).await {
                Ok(_) => client.delete_project_dir(&old).await.map(|_| ()),
                Err(error) => Err(error),
            };
            if cleared.is_err() {
                // The old folder is not provably gone: restore it as usual.
                return Err(restore_previous(client, error, &old, &storage_id, progress).await);
            }
            progress(format!("Sending the project to {old}"), Some(50));
            match write_and_verify(client, &old, files, expected_hash, true, progress).await {
                Ok(hash) => {
                    progress("Done".to_string(), Some(100));
                    Ok(PushReport {
                        storage_id: old,
                        hash,
                    })
                }
                Err(error) => {
                    let _ = client.delete_project_dir(&old).await;
                    Err(match is_no_space(&error) {
                        true => ClientError::Server(format!(
                            "this board can't hold two copies of the project, so the old copy \
                             ({old}) was removed to make room — and the project still doesn't \
                             fit on the board"
                        )),
                        false => with_note(
                            error,
                            &format!(
                                "the previous project ({old}) was removed to make room, so \
                                 the board is running nothing"
                            ),
                        ),
                    })
                }
            }
        }
        (Err(error), Some(old)) if old != storage_id => {
            Err(restore_previous(client, error, &old, &storage_id, progress).await)
        }
        (Err(error), _) => Err(error),
    }
}

/// Write `files` into `storage_id` (deflated where that shrinks them; no
/// batch: this board has none), load it, and check the board holds exactly
/// the library's bytes. `stop` sends a `StopAllProjects` first.
async fn write_and_verify<Io: ClientIo>(
    client: &mut LpClient<Io>,
    storage_id: &str,
    files: &[(String, Vec<u8>)],
    expected_hash: &str,
    stop: bool,
    progress: PushProgress<'_>,
) -> ClientResult<String> {
    let plan = DeployPlan {
        stop,
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
    verify_hash(client, storage_id, expected_hash).await
}

/// The old dir was never touched: put it back on, then drop what the board
/// refused. The error is the refusal, with what became of the board appended
/// — the card must not guess.
async fn restore_previous<Io: ClientIo>(
    client: &mut LpClient<Io>,
    error: ClientError,
    old: &str,
    storage_id: &str,
    progress: PushProgress<'_>,
) -> ClientError {
    progress(
        match is_no_space(&error) {
            true => "The board is out of room for a second copy; restoring what it ran",
            false => "The board refused it; restoring what it ran",
        }
        .to_string(),
        Some(90),
    );
    let restored = client
        .project_load(&crate::project_deploy::project_load_path(old))
        .await;
    let _ = client.delete_project_dir(storage_id).await;
    with_restore_note(error, old, restored.is_ok())
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::device_push::push_project;
    use crate::device_push::test_script::*;
    use crate::scripted_io::ScriptedIo;

    #[test]
    fn a_push_writes_the_slot_the_board_does_not_run_from() {
        assert_eq!(other_slot("demo"), "demo-b");
        assert_eq!(other_slot("demo-b"), "demo");
        assert_eq!(other_slot("2026-08-30-porch"), "2026-08-30-porch-b");
        // A dir literally named "-b" has no base; it alternates with "-b-b".
        assert_eq!(other_slot("-b"), "-b-b");
    }

    /// (a) Room for two copies: the push is the two-slot push it always was —
    /// new slot written and verified, then the old one removed.
    #[tokio::test]
    async fn a_push_with_room_writes_the_other_slot_then_removes_the_old_one() {
        let io = ScriptedIo::new([
            loaded(1, "demo"),
            stopped(2),
            batch(3, lpc_wire::BatchOp::Begin, false),
            deleted(4),
            written(5, None),
            written(6, None),
            loaded_ok(7),
            hashed(8, HASH),
            deleted(9),
        ]);
        let mut client = LpClient::new(io);
        let mut noted: Vec<String> = Vec::new();
        let mut progress = |label: String, _percent: Option<u8>| noted.push(label);

        let report = push_project(&mut client, &files(), HASH, "fallback", &mut progress)
            .await
            .expect("pushed");

        assert_eq!(report.storage_id, "demo-b");
        assert_eq!(
            ops(&client.into_io().sent),
            [
                "list-loaded",
                "stop",
                "begin",
                "delete demo-b",
                "write /projects/demo-b/project.json",
                "write /projects/demo-b/main.glsl",
                "load projects/demo-b",
                "hash /projects/demo-b",
                "delete demo",
            ]
        );
        assert!(!noted.iter().any(|label| label.contains("two copies")));
    }

    /// (b) The board runs out of room for the second copy: the partial new
    /// slot and the old one go, and the project is written again into the OLD
    /// folder's own name — so a power cut mid-way leaves `startup_project`
    /// naming a folder the next push replaces normally.
    #[tokio::test]
    async fn no_room_for_two_copies_replaces_the_old_slot_in_place() {
        let io = ScriptedIo::new([
            loaded(1, "demo"),
            stopped(2),
            batch(3, lpc_wire::BatchOp::Begin, false),
            deleted(4),
            written(5, None),
            written(6, Some(NO_SPACE)),
            // The wipe: partial new slot, then the old one.
            deleted(7),
            deleted(8),
            // The rewrite, into `demo`.
            stopped(9),
            deleted(10),
            written(11, None),
            written(12, None),
            loaded_ok(13),
            hashed(14, HASH),
        ]);
        let mut client = LpClient::new(io);
        let mut noted: Vec<String> = Vec::new();
        let mut progress = |label: String, _percent: Option<u8>| noted.push(label);

        let report = push_project(&mut client, &files(), HASH, "fallback", &mut progress)
            .await
            .expect("pushed in place");

        assert_eq!(report.storage_id, "demo", "the old folder's own name");
        assert_eq!(report.hash, HASH);
        assert_eq!(
            ops(&client.into_io().sent),
            [
                "list-loaded",
                "stop",
                "begin",
                "delete demo-b",
                "write /projects/demo-b/project.json",
                "write /projects/demo-b/main.glsl",
                "delete demo-b",
                "delete demo",
                "stop",
                "delete demo",
                "write /projects/demo/project.json",
                "write /projects/demo/main.glsl",
                "load projects/demo",
                "hash /projects/demo",
            ]
        );
        assert!(
            noted
                .iter()
                .any(|label| label.contains("can't hold two copies") && label.contains("demo")),
            "the card says why the old copy is going: {noted:?}"
        );
    }

    /// (c) Too big even alone: the old copy is gone, and the error says so in
    /// plain words instead of "the board refused it".
    #[tokio::test]
    async fn a_project_too_big_even_alone_says_the_old_copy_was_removed() {
        let io = ScriptedIo::new([
            loaded(1, "demo"),
            stopped(2),
            batch(3, lpc_wire::BatchOp::Begin, false),
            deleted(4),
            written(5, Some(NO_SPACE)),
            deleted(6),
            deleted(7),
            stopped(8),
            deleted(9),
            written(10, Some(NO_SPACE)),
            // Best-effort cleanup of the partial rewrite.
            deleted(11),
        ]);
        let mut client = LpClient::new(io);
        let mut noted: Vec<String> = Vec::new();
        let mut progress = |label: String, _percent: Option<u8>| noted.push(label);

        let error = push_project(&mut client, &files(), HASH, "fallback", &mut progress)
            .await
            .expect_err("does not fit");

        let message = error.to_string();
        assert!(message.contains("demo"), "{message}");
        assert!(message.contains("removed to make room"), "{message}");
        assert!(message.contains("still doesn't fit"), "{message}");
        assert!(!message.contains("refused"), "{message}");
        assert!(
            !noted.iter().any(|label| label.contains("restoring")),
            "there is nothing to restore: {noted:?}"
        );
    }

    /// (d) Any other failure keeps today's path: the old folder was never
    /// touched, so it is loaded again and the refused slot dropped.
    #[tokio::test]
    async fn a_failure_that_is_not_about_space_restores_the_previous_project() {
        let io = ScriptedIo::new([
            loaded(1, "demo"),
            stopped(2),
            batch(3, lpc_wire::BatchOp::Begin, false),
            deleted(4),
            written(5, Some("corrupt block")),
            loaded_ok(6),
            deleted(7),
        ]);
        let mut client = LpClient::new(io);
        let mut noted: Vec<String> = Vec::new();
        let mut progress = |label: String, _percent: Option<u8>| noted.push(label);

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
                "delete demo-b",
                "write /projects/demo-b/project.json",
                "load projects/demo",
                "delete demo-b",
            ]
        );
        assert!(noted.iter().any(|label| label.contains("refused it")));
    }
}
