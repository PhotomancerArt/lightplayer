//! **The project stage's edge** (M3 P11): `lpa-update`'s sans-IO
//! [`ProjectStage`] run over a board's channel 1, one request at a time, by
//! whichever client a host holds ([`FileRequestSink`]) — Studio's
//! conversation on a device's shared link, `lp-cli`'s client on every route.
//! One edge, so every host upgrades a board's projects with the same
//! requests.
//!
//! | Effect | Requests |
//! |---|---|
//! | `ListProjects` | `ListAvailableProjects` |
//! | `ReadStartupChoice` | `Read /lightplayer.json` (refused at play, missing on a fresh board: none) |
//! | `ReadManifest` | `Read <project>/project.json` |
//! | `ReadProject` | `ListDir <project>` recursive, then a `Read` of every file |
//! | `WriteBatch` | `BeginBatch`; `atomic: false` → nothing written; else the push's own writes (deflated where that shrinks a file), `DeleteFile` each, `CommitBatch` (an error aborts) |
//! | `LoadProject` | `LoadProject projects/<name>` |
//!
//! A request that fails without an answer (the link went) ends the stage
//! `Interrupted`: every project is wholly old or wholly new, and the next run
//! starts over from the board. Channel 1 only: the update channel is never
//! touched here.

use lpa_update::{BatchOutcome, ProjectStage, ProjectStageEffect, ProjectStageReport};
use lpa_upgrade::ProjectFiles;
use lpc_model::AsLpPathBuf;
use lpc_model::server::server_config::ServerConfig;
use lpc_wire::server::FsResponse;
use lpc_wire::{ClientRequest, FsRequest, WireServerMsgBody};

use crate::client_error::{ClientError, ClientResult};
use crate::project_deploy::{project_file_path, project_load_path};
use crate::push_files::{
    FileRequestSink, abort_batch, begin_batch, commit_batch, expect_written, file_requests,
};

/// How far the rewriting is, counted in projects older than the board's
/// engine format: `done` of `total`. Said before each project is read and
/// once more when the stage ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectStageProgress {
    pub done: usize,
    pub total: usize,
}

/// Run `stage` to its end over `sink`, saying how far it is as it goes.
pub async fn run_project_stage(
    sink: &mut dyn FileRequestSink,
    stage: &mut ProjectStage,
    on_progress: &mut dyn FnMut(ProjectStageProgress),
) -> ProjectStageReport {
    let mut effects = stage.start();
    loop {
        let Some(effect) = effects.pop() else {
            // The stage always asks for something until it is done; an
            // empty answer is a stage fed out of turn, which this edge never
            // does. End rather than spin.
            return ProjectStageReport {
                end: lpa_update::StageEnd::Interrupted(
                    "the project stage stopped asking".to_string(),
                ),
                ..ProjectStageReport::default()
            };
        };
        if let Some((done, total)) = stage.progress() {
            on_progress(ProjectStageProgress { done, total });
        }
        effects = match effect {
            ProjectStageEffect::Done(report) => {
                let total = report.upgraded.len() + report.pending.len();
                on_progress(ProjectStageProgress {
                    done: report.upgraded.len(),
                    total,
                });
                return report;
            }
            ProjectStageEffect::ListProjects => {
                match sink.exchange(ClientRequest::ListAvailableProjects).await {
                    Ok(WireServerMsgBody::ListAvailableProjects { projects }) => stage
                        .projects_listed(
                            projects
                                .iter()
                                .map(|project| String::from(project.path.as_str()))
                                .collect(),
                        ),
                    Ok(other) => stage.read_failed(
                        ClientError::unexpected_response("project.list_available", other)
                            .to_string(),
                    ),
                    Err(error) => stage.read_failed(error.to_string()),
                }
            }
            ProjectStageEffect::ReadStartupChoice => match read(sink, ServerConfig::PATH).await {
                Ok(bytes) => stage.startup_choice_read(bytes),
                // A play link may not read outside the projects directory:
                // the order is then the board's own fallback, the first by
                // path. Nothing is written at play anyway.
                Err(ClientError::NotPermitted { .. }) => stage.startup_choice_read(None),
                Err(error) => stage.read_failed(error.to_string()),
            },
            ProjectStageEffect::ReadManifest { project } => {
                let path = project_file_path(project_name(&project), "project.json");
                match read(sink, &path).await {
                    Ok(bytes) => stage.manifest_read(&project, bytes),
                    Err(error) => stage.read_failed(error.to_string()),
                }
            }
            ProjectStageEffect::ReadProject { project } => {
                match read_project(sink, project_name(&project)).await {
                    Ok(files) => stage.project_read(&project, files),
                    Err(error) => stage.read_failed(error.to_string()),
                }
            }
            ProjectStageEffect::WriteBatch {
                project,
                writes,
                deletes,
            } => {
                let outcome = write_batch(sink, project_name(&project), &writes, &deletes).await;
                stage.batch_done(&project, outcome)
            }
            ProjectStageEffect::LoadProject { project } => {
                let request = ClientRequest::LoadProject {
                    path: project_load_path(project_name(&project)),
                };
                let result = match sink.exchange(request).await {
                    Ok(WireServerMsgBody::LoadProject { .. }) => Ok(()),
                    Ok(WireServerMsgBody::Error { error }) => Err(error),
                    Ok(other) => Err(format!("{other:?}")),
                    Err(ClientError::Server(error)) => Err(error),
                    Err(error) => {
                        // No answer: the link went. The project is current
                        // either way; the next run finishes the rest.
                        effects = stage.link_lost();
                        log::info!("project stage: the load went unanswered: {error}");
                        continue;
                    }
                };
                stage.load_done(&project, result)
            }
        };
    }
}

/// The project's directory name: the last segment of a listed path, which is
/// what the push path's `/projects/<name>` paths are built from.
fn project_name(project: &str) -> &str {
    let trimmed = project.trim_end_matches('/');
    trimmed.rsplit('/').next().unwrap_or(trimmed)
}

/// `Read path`: its bytes, or `None` when the board has no such file.
async fn read(sink: &mut dyn FileRequestSink, path: &str) -> ClientResult<Option<Vec<u8>>> {
    let body = sink
        .exchange(ClientRequest::Filesystem(FsRequest::Read {
            path: path.as_path_buf(),
        }))
        .await;
    match body {
        Ok(WireServerMsgBody::Filesystem(FsResponse::Read { data, error, .. })) => match error {
            None => Ok(data),
            // The board answered: the file is not there (or not readable as
            // a file). A missing manifest or startup choice is a fact the
            // stage judges, not a broken link.
            Some(_) => Ok(None),
        },
        Ok(other) => Err(ClientError::unexpected_response("fs.read", other)),
        Err(ClientError::Server(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Every file under `/projects/<name>`, keyed by its path inside the
/// project: `ListDir` recursive, then a `Read` of every entry that is not a
/// directory (an entry another entry lies under).
async fn read_project(sink: &mut dyn FileRequestSink, name: &str) -> ClientResult<ProjectFiles> {
    let root = format!("/projects/{name}");
    let body = sink
        .exchange(ClientRequest::Filesystem(FsRequest::ListDir {
            path: root.as_str().as_path_buf(),
            recursive: true,
        }))
        .await?;
    let entries: Vec<String> = match body {
        WireServerMsgBody::Filesystem(FsResponse::ListDir {
            entries,
            error: None,
            ..
        }) => entries
            .iter()
            .map(|entry| String::from(entry.as_str()))
            .collect(),
        WireServerMsgBody::Filesystem(FsResponse::ListDir {
            error: Some(error), ..
        }) => return Err(ClientError::Server(format!("listing {root}: {error}"))),
        other => return Err(ClientError::unexpected_response("fs.list_dir", other)),
    };
    let prefix = format!("{root}/");
    let mut files = ProjectFiles::new();
    for entry in &entries {
        let dir = format!("{}/", entry.trim_end_matches('/'));
        if entries.iter().any(|other| other.starts_with(&dir)) {
            continue;
        }
        let Some(key) = entry.strip_prefix(&prefix) else {
            continue;
        };
        match read(sink, entry).await? {
            Some(bytes) => {
                files.insert(key, bytes);
            }
            None => {
                return Err(ClientError::Server(format!(
                    "{entry} was listed and could not be read"
                )));
            }
        }
    }
    Ok(files)
}

/// One atomic rewrite of `/projects/<name>`: nothing at all on a board whose
/// `BeginBatch` is not atomic.
async fn write_batch(
    sink: &mut dyn FileRequestSink,
    name: &str,
    writes: &[(String, Vec<u8>)],
    deletes: &[String],
) -> BatchOutcome {
    match begin_batch(sink).await {
        Ok(true) => {}
        // No transaction opened, so nothing may be written (Q109).
        Ok(false) => return BatchOutcome::NotAtomic,
        Err(error) => return BatchOutcome::Failed(error.to_string()),
    }
    match write_in_batch(sink, name, writes, deletes).await {
        Ok(()) => match commit_batch(sink).await {
            Ok(()) => BatchOutcome::Committed,
            Err(error) => BatchOutcome::Failed(error.to_string()),
        },
        Err(error) => {
            // Best effort: a request that went unanswered may never have
            // arrived, and the board drops a batch whose link closes anyway.
            let _ = abort_batch(sink).await;
            BatchOutcome::Failed(error.to_string())
        }
    }
}

async fn write_in_batch(
    sink: &mut dyn FileRequestSink,
    name: &str,
    writes: &[(String, Vec<u8>)],
    deletes: &[String],
) -> ClientResult<()> {
    for (path, bytes) in writes {
        for request in file_requests(name, path, bytes, true) {
            let body = sink.exchange(request.clone()).await?;
            expect_written(&request, body)?;
        }
    }
    for path in deletes {
        let full = project_file_path(name, path);
        let body = sink
            .exchange(ClientRequest::Filesystem(FsRequest::DeleteFile {
                path: full.as_str().as_path_buf(),
            }))
            .await?;
        match body {
            WireServerMsgBody::Filesystem(FsResponse::DeleteFile { error: None, .. }) => {}
            WireServerMsgBody::Filesystem(FsResponse::DeleteFile {
                error: Some(error), ..
            }) => {
                return Err(ClientError::Server(format!(
                    "failed to delete {full}: {error}"
                )));
            }
            other => return Err(ClientError::unexpected_response("fs.delete_file", other)),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeMap;

    use async_trait::async_trait;
    use lpc_wire::WireProjectHandle;
    use lpc_wire::server::{AvailableProject, BatchOp};

    #[test]
    fn a_boards_older_project_is_upgraded_in_one_batch_and_loaded() {
        let mut board = MemoryBoard::with_project("eaves", 11);
        let mut stage = toy_stage();
        let mut seen = Vec::new();
        let report = block_on(run_project_stage(&mut board, &mut stage, &mut |p| {
            seen.push((p.done, p.total))
        }));

        assert_eq!(report.end, lpa_update::StageEnd::Finished, "{report:?}");
        assert_eq!(report.upgraded.len(), 1);
        assert_eq!(report.load, Some(Ok(())));
        assert_eq!(
            board.files["/projects/eaves/project.json"],
            br#"{"format": 12}"#
        );
        assert_eq!(
            board.log,
            ["begin", "write", "commit", "load projects/eaves"]
        );
        assert_eq!(seen.first(), Some(&(0, 1)));
        assert_eq!(seen.last(), Some(&(1, 1)));
    }

    #[test]
    fn a_board_without_transactions_is_not_written() {
        let mut board = MemoryBoard::with_project("eaves", 11);
        board.atomic = false;
        let mut stage = toy_stage();
        let report = block_on(run_project_stage(&mut board, &mut stage, &mut |_| {}));

        assert_eq!(report.end, lpa_update::StageEnd::NotAtomic);
        assert_eq!(board.log, ["begin"]);
        assert_eq!(
            board.files["/projects/eaves/project.json"],
            br#"{"format": 11}"#
        );
    }

    #[test]
    fn a_link_that_goes_mid_batch_leaves_the_project_as_it_was() {
        let mut board = MemoryBoard::with_project("eaves", 11);
        board.silent_after = Some(6);
        let mut stage = toy_stage();
        let report = block_on(run_project_stage(&mut board, &mut stage, &mut |_| {}));

        assert!(
            matches!(report.end, lpa_update::StageEnd::Interrupted(_)),
            "{report:?}"
        );
        assert_eq!(report.pending, ["projects/eaves"]);
        assert_eq!(
            board.files["/projects/eaves/project.json"],
            br#"{"format": 11}"#
        );
    }

    // --- the board -----------------------------------------------------------------

    /// A stage at edit over a toy chain (11 → 12 sets the format), so these
    /// tests are about the edge, not about any real step.
    fn toy_stage() -> ProjectStage {
        ProjectStage::with_chain(
            12,
            Some(lpc_access::Tier::Edit),
            vec![lpa_upgrade::UpgradeStep::new(11, 12, toy_11_to_12)],
        )
    }

    fn toy_11_to_12(
        files: &mut ProjectFiles,
        report: &mut lpa_upgrade::UpgradeReport,
    ) -> Result<(), lpa_upgrade::UpgradeError> {
        files.insert(lpa_upgrade::PROJECT_MANIFEST, br#"{"format": 12}"#.to_vec());
        report.record_changed(lpa_upgrade::PROJECT_MANIFEST);
        Ok(())
    }

    /// One board's channel 1 over a map of files: a batch is staged and
    /// lands at its commit.
    struct MemoryBoard {
        files: BTreeMap<String, Vec<u8>>,
        staged: Option<BTreeMap<String, Vec<u8>>>,
        atomic: bool,
        log: Vec<String>,
        requests: usize,
        silent_after: Option<usize>,
    }

    impl MemoryBoard {
        fn with_project(name: &str, format: u32) -> Self {
            let mut files = BTreeMap::new();
            files.insert(
                format!("/projects/{name}/project.json"),
                format!("{{\"format\": {format}}}").into_bytes(),
            );
            Self {
                files,
                staged: None,
                atomic: true,
                log: Vec::new(),
                requests: 0,
                silent_after: None,
            }
        }

        fn view(&self) -> &BTreeMap<String, Vec<u8>> {
            self.staged.as_ref().unwrap_or(&self.files)
        }
    }

    #[async_trait(?Send)]
    impl FileRequestSink for MemoryBoard {
        async fn exchange(&mut self, request: ClientRequest) -> ClientResult<WireServerMsgBody> {
            self.requests += 1;
            if self.silent_after.is_some_and(|n| self.requests > n) {
                self.staged = None;
                return Err(ClientError::Transport("the link went".to_string()));
            }
            Ok(match request {
                ClientRequest::ListAvailableProjects => {
                    let mut names: Vec<String> = self
                        .files
                        .keys()
                        .filter_map(|path| path.strip_suffix("/project.json"))
                        .map(|dir| dir.trim_start_matches('/').to_string())
                        .collect();
                    names.dedup();
                    WireServerMsgBody::ListAvailableProjects {
                        projects: names
                            .into_iter()
                            .map(|path| AvailableProject {
                                path: path.as_str().as_path_buf(),
                            })
                            .collect(),
                    }
                }
                ClientRequest::Filesystem(FsRequest::Read { path }) => {
                    let data = self.view().get(path.as_str()).cloned();
                    let error = data.is_none().then(|| "not found".to_string());
                    WireServerMsgBody::Filesystem(FsResponse::Read { path, data, error })
                }
                ClientRequest::Filesystem(FsRequest::ListDir { path, .. }) => {
                    let prefix = format!("{}/", path.as_str());
                    let entries = self
                        .view()
                        .keys()
                        .filter(|key| key.starts_with(&prefix))
                        .map(|key| key.as_str().as_path_buf())
                        .collect();
                    WireServerMsgBody::Filesystem(FsResponse::ListDir {
                        path,
                        entries,
                        error: None,
                    })
                }
                ClientRequest::Filesystem(FsRequest::BeginBatch) => {
                    self.log.push("begin".to_string());
                    if self.atomic {
                        self.staged = Some(self.files.clone());
                    }
                    WireServerMsgBody::Filesystem(FsResponse::Batch {
                        op: BatchOp::Begin,
                        atomic: self.atomic,
                        error: None,
                    })
                }
                ClientRequest::Filesystem(FsRequest::Write { path, data }) => {
                    self.log.push("write".to_string());
                    let staged = self.staged.as_mut().expect("inside a batch");
                    staged.insert(path.as_str().to_string(), data);
                    WireServerMsgBody::Filesystem(FsResponse::Write { path, error: None })
                }
                ClientRequest::Filesystem(FsRequest::CommitBatch) => {
                    self.log.push("commit".to_string());
                    self.files = self.staged.take().expect("inside a batch");
                    WireServerMsgBody::Filesystem(FsResponse::Batch {
                        op: BatchOp::Commit,
                        atomic: true,
                        error: None,
                    })
                }
                ClientRequest::Filesystem(FsRequest::AbortBatch) => {
                    self.staged = None;
                    WireServerMsgBody::Filesystem(FsResponse::Batch {
                        op: BatchOp::Abort,
                        atomic: true,
                        error: None,
                    })
                }
                ClientRequest::LoadProject { path } => {
                    self.log.push(format!("load {path}"));
                    WireServerMsgBody::LoadProject {
                        handle: WireProjectHandle::new(1),
                    }
                }
                other => panic!("the stage asked for {other:?}"),
            })
        }
    }

    /// Every future here is ready at once (the board answers in-line).
    fn block_on<F: core::future::Future>(future: F) -> F::Output {
        use core::pin::pin;
        use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
        fn raw() -> RawWaker {
            fn clone(_: *const ()) -> RawWaker {
                raw()
            }
            fn noop(_: *const ()) {}
            static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
            RawWaker::new(core::ptr::null(), &VTABLE)
        }
        // SAFETY: the vtable's functions do nothing and the data is null.
        let waker = unsafe { Waker::from_raw(raw()) };
        let mut context = Context::from_waker(&waker);
        let mut future = pin!(future);
        loop {
            if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
                return output;
            }
        }
    }
}
