//! The one way a project's files reach a board: the shared push primitive.
//!
//! Every path that writes a project — Studio's device push
//! ([`crate::push_project`]), the library open
//! ([`crate::LpClient::replace_and_load_project_observed`]), `lp-cli
//! upload`/`dev` and every other `deploy_project_files` caller — runs
//! [`deploy_files`], over whichever client it holds ([`FileRequestSink`]).
//! It is strictly request/response, one request in flight.
//!
//! ```text
//! [StopAll] → [BeginBatch] → [DeleteDir] → writes → [LoadProject]
//!           → [HashPackage check] → [CommitBatch]
//! ```
//!
//! - **The batch.** A board whose filesystem has transactions answers
//!   `BeginBatch` with `atomic: true`, and everything after it lands at the
//!   `CommitBatch` or not at all; the commit comes after the load and the
//!   hash, so the board commits only a project it has loaded and holds
//!   exactly. Any error after the begin aborts (best effort: a request that
//!   timed out may never have arrived, so nothing relies on it), and the
//!   board is left as it was. A board without transactions answers
//!   `atomic: false`, opens nothing, and every write lands by itself.
//! - **Deflated writes.** A file is planned with `lp_tree_store`'s
//!   hasher-free [`plan_deflated_chunks`] (`miniz_oxide`, level 10) for the
//!   wire's constant record hint ([`FILE_SYNC_RECORD_MAX_HINT`]): chunks of
//!   at most 4 KiB logical, each one stored record on a tree-store board,
//!   sent as `WriteChunkDeflated` at logical offsets. A file under
//!   [`DEFLATE_MIN_BYTES`], or one that does not shrink by a tenth, goes as
//!   it always has (`Write`, or `WriteChunk` runs).
//!
//! `device_stamp`'s journaled `/hardware.json` writes are not a project and
//! do not come here.

use async_trait::async_trait;
use lp_tree_store::{DEFAULT_DEFLATE_LEVEL, plan_deflated_chunks};
use lpc_model::AsLpPathBuf;
use lpc_wire::budget::FILE_SYNC_RECORD_MAX_HINT;
use lpc_wire::server::{BatchOp, FsResponse};
use lpc_wire::{ClientRequest, FsRequest, WireProjectHandle, WireServerMsgBody};

use crate::client::{DeployStep, LpClient};
use crate::client_error::{ClientError, ClientResult};
use crate::client_event::ClientEvent;
use crate::client_io::ClientIo;
use crate::project_deploy::{project_file_path, project_load_path};

/// A file smaller than this goes as it is: deflating it saves less than
/// the request's own envelope.
pub const DEFLATE_MIN_BYTES: usize = 64;

/// The client a push runs over: one request out, its answer back.
#[async_trait(?Send)]
pub trait FileRequestSink {
    /// Send `request` and return the body of its answer (a server error or a
    /// tier refusal is an `Err`).
    async fn exchange(&mut self, request: ClientRequest) -> ClientResult<WireServerMsgBody>;
}

/// An [`LpClient`] as a [`FileRequestSink`], keeping the events it heard
/// along the way.
pub struct LpClientSink<'a, Io> {
    pub client: &'a mut LpClient<Io>,
    pub events: Vec<ClientEvent>,
}

impl<'a, Io> LpClientSink<'a, Io> {
    pub fn new(client: &'a mut LpClient<Io>) -> Self {
        Self {
            client,
            events: Vec::new(),
        }
    }
}

#[async_trait(?Send)]
impl<Io: ClientIo> FileRequestSink for LpClientSink<'_, Io> {
    async fn exchange(&mut self, request: ClientRequest) -> ClientResult<WireServerMsgBody> {
        let outcome = self.client.send_request(request).await?;
        self.events.extend(outcome.events);
        Ok(outcome.value.msg)
    }
}

/// What [`deploy_files`] does around the writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeployPlan<'a> {
    /// Send `StopAllProjects` first.
    pub stop: bool,
    /// The batch around everything after the stop.
    pub batch: BatchUse,
    /// Delete the project's directory before writing (a replace, not a
    /// write-over).
    pub clear: bool,
    /// Deflate what shrinks.
    pub deflate: bool,
    /// Send `LoadProject` after the writes.
    pub load: bool,
    /// Check the board's package hash against this before committing.
    pub expected_hash: Option<&'a str>,
}

/// Whether, and how, a deploy runs inside a batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchUse {
    /// No batch: every write lands by itself.
    None,
    /// Send `BeginBatch`, and commit if the board answers `atomic`.
    Begin,
    /// The caller began one and the board answered `atomic: true`: commit it
    /// (or abort it) here.
    Open,
}

/// What a deploy did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeployOutcome {
    /// The board committed it as one batch.
    pub atomic: bool,
    /// The loaded project's handle, when the plan loaded it.
    pub handle: Option<WireProjectHandle>,
    /// The board's package hash, when the plan checked it.
    pub hash: Option<String>,
}

/// Run `plan` for `files` into `/projects/<project_id>`, reporting each
/// step as it starts and every write as it lands.
pub async fn deploy_files(
    sink: &mut dyn FileRequestSink,
    project_id: &str,
    files: &[(String, Vec<u8>)],
    plan: DeployPlan<'_>,
    on_step: &mut dyn FnMut(DeployStep),
) -> ClientResult<DeployOutcome> {
    if plan.stop || plan.clear {
        on_step(DeployStep::Clearing);
    }
    if plan.stop {
        expect_stopped(sink.exchange(ClientRequest::StopAllProjects).await?)?;
    }
    let atomic = match plan.batch {
        BatchUse::None => false,
        BatchUse::Begin => begin_batch(sink).await?,
        BatchUse::Open => true,
    };
    let landed = run_in_batch(sink, project_id, files, plan, on_step).await;
    match landed {
        Ok((handle, hash)) => {
            if atomic {
                commit_batch(sink).await?;
            }
            Ok(DeployOutcome {
                atomic,
                handle,
                hash,
            })
        }
        Err(error) => {
            if atomic {
                let _ = abort_batch(sink).await;
            }
            Err(error)
        }
    }
}

/// `BeginBatch`: whether the board's filesystem commits it as one.
pub async fn begin_batch(sink: &mut dyn FileRequestSink) -> ClientResult<bool> {
    let body = sink
        .exchange(ClientRequest::Filesystem(FsRequest::BeginBatch))
        .await?;
    batch_answer(body, BatchOp::Begin)
}

/// `CommitBatch`.
pub async fn commit_batch(sink: &mut dyn FileRequestSink) -> ClientResult<()> {
    let body = sink
        .exchange(ClientRequest::Filesystem(FsRequest::CommitBatch))
        .await?;
    batch_answer(body, BatchOp::Commit).map(drop)
}

/// `AbortBatch` (harmless with none open).
pub async fn abort_batch(sink: &mut dyn FileRequestSink) -> ClientResult<()> {
    let body = sink
        .exchange(ClientRequest::Filesystem(FsRequest::AbortBatch))
        .await?;
    batch_answer(body, BatchOp::Abort).map(drop)
}

/// The requests that write `bytes` to `relative_path` in the project:
/// deflated chunks when `deflate` and the file shrinks, else the plain
/// `Write` / `WriteChunk` form.
pub fn file_requests(
    project_id: &str,
    relative_path: &str,
    bytes: &[u8],
    deflate: bool,
) -> Vec<ClientRequest> {
    let plain = || crate::file_sync_ops::file_write_requests(project_id, relative_path, bytes);
    if !deflate || bytes.len() < DEFLATE_MIN_BYTES {
        return plain();
    }
    let chunks = plan_deflated_chunks(bytes, FILE_SYNC_RECORD_MAX_HINT, DEFAULT_DEFLATE_LEVEL);
    let deflated: usize = chunks.iter().map(|chunk| chunk.deflated.len()).sum();
    // Less than a tenth smaller is not worth the board's inflate.
    if deflated * 10 > bytes.len() * 9 {
        return plain();
    }
    let path = project_file_path(project_id, relative_path);
    chunks
        .into_iter()
        .map(|chunk| {
            ClientRequest::Filesystem(FsRequest::WriteChunkDeflated {
                path: path.as_str().as_path_buf(),
                offset: chunk.logical_range.start as u32,
                logical_len: chunk.logical_range.len() as u32,
                data: chunk.deflated,
            })
        })
        .collect()
}

/// The logical bytes a write request carries (what a progress bar counts).
pub fn logical_bytes(request: &ClientRequest) -> u64 {
    match request {
        ClientRequest::Filesystem(FsRequest::Write { data, .. })
        | ClientRequest::Filesystem(FsRequest::WriteChunk { data, .. }) => data.len() as u64,
        ClientRequest::Filesystem(FsRequest::WriteChunkDeflated { logical_len, .. }) => {
            u64::from(*logical_len)
        }
        _ => 0,
    }
}

/// Everything after the begin and before the commit.
async fn run_in_batch(
    sink: &mut dyn FileRequestSink,
    project_id: &str,
    files: &[(String, Vec<u8>)],
    plan: DeployPlan<'_>,
    on_step: &mut dyn FnMut(DeployStep),
) -> ClientResult<(Option<WireProjectHandle>, Option<String>)> {
    if plan.clear {
        clear_project_dir(sink, project_id).await?;
    }
    let total_bytes: u64 = files.iter().map(|(_, bytes)| bytes.len() as u64).sum();
    let mut sent_bytes = 0u64;
    on_step(DeployStep::Writing {
        sent_bytes,
        total_bytes,
    });
    for (relative_path, bytes) in files {
        for request in file_requests(project_id, relative_path, bytes, plan.deflate) {
            let written = logical_bytes(&request);
            let body = sink.exchange(request.clone()).await?;
            expect_written(&request, body)?;
            sent_bytes += written;
            on_step(DeployStep::Writing {
                sent_bytes,
                total_bytes,
            });
        }
    }
    let handle = match plan.load {
        true => {
            on_step(DeployStep::Loading);
            let body = sink
                .exchange(ClientRequest::LoadProject {
                    path: project_load_path(project_id),
                })
                .await?;
            match body {
                WireServerMsgBody::LoadProject { handle } => Some(handle),
                other => return Err(ClientError::unexpected_response("project.load", other)),
            }
        }
        false => None,
    };
    let hash = match plan.expected_hash {
        Some(expected) => {
            let body = sink
                .exchange(crate::file_sync_ops::hash_package_request(project_id))
                .await?;
            let hash = crate::file_sync_ops::validate_hash_package_response(&body)?;
            if hash != expected {
                // The bytes on the board are not the bytes in the library.
                // Saying so beats a green card over a project nobody has.
                return Err(ClientError::Protocol(format!(
                    "the board ended up with different bytes than the library sent \
                     (device {hash}, library {expected}) — the project was not \
                     fully written"
                )));
            }
            Some(hash)
        }
        None => None,
    };
    Ok((handle, hash))
}

/// Delete `/projects/<project_id>`; an absent directory is success
/// (`LpClient::delete_project_dir`'s rule).
pub async fn clear_project_dir(
    sink: &mut dyn FileRequestSink,
    project_id: &str,
) -> ClientResult<()> {
    let prefix = format!("/projects/{project_id}");
    let body = sink
        .exchange(ClientRequest::Filesystem(FsRequest::DeleteDir {
            path: prefix.as_str().as_path_buf(),
        }))
        .await?;
    match body {
        WireServerMsgBody::Filesystem(FsResponse::DeleteDir { error: None, .. }) => Ok(()),
        WireServerMsgBody::Filesystem(FsResponse::DeleteDir {
            error: Some(error), ..
        }) => {
            // fs errors cross the wire as display strings.
            if error.starts_with("File not found") || error.contains("no such file or directory") {
                Ok(())
            } else {
                Err(ClientError::Server(format!(
                    "failed to clear {prefix}: {error}"
                )))
            }
        }
        other => Err(ClientError::unexpected_response("fs.delete_dir", other)),
    }
}

fn expect_stopped(body: WireServerMsgBody) -> ClientResult<()> {
    match body {
        WireServerMsgBody::StopAllProjects => Ok(()),
        other => Err(ClientError::unexpected_response("project.stop_all", other)),
    }
}

fn expect_written(request: &ClientRequest, body: WireServerMsgBody) -> ClientResult<()> {
    let path = match request {
        ClientRequest::Filesystem(
            FsRequest::Write { path, .. }
            | FsRequest::WriteChunk { path, .. }
            | FsRequest::WriteChunkDeflated { path, .. },
        ) => path.as_str(),
        _ => "?",
    };
    match body {
        WireServerMsgBody::Filesystem(
            FsResponse::Write { error, .. } | FsResponse::WriteChunk { error, .. },
        ) => match error {
            None => Ok(()),
            Some(error) => Err(ClientError::Server(format!(
                "failed to write {path}: {error}"
            ))),
        },
        other => Err(ClientError::unexpected_response(
            crate::project_deploy::request_label(request),
            other,
        )),
    }
}

fn batch_answer(body: WireServerMsgBody, op: BatchOp) -> ClientResult<bool> {
    match body {
        WireServerMsgBody::Filesystem(FsResponse::Batch {
            op: answered,
            atomic,
            error,
        }) if answered == op => match error {
            None => Ok(atomic),
            Some(error) => Err(ClientError::Server(error)),
        },
        other => Err(ClientError::unexpected_response(
            match op {
                BatchOp::Begin => "fs.begin_batch",
                BatchOp::Commit => "fs.commit_batch",
                BatchOp::Abort => "fs.abort_batch",
            },
            other,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_push::test_script::*;
    use crate::scripted_io::ScriptedIo;

    /// The library open on a board with batches: in place, one batch,
    /// committed after the load.
    #[tokio::test]
    async fn the_library_open_is_one_batch_on_an_atomic_board() {
        let io = ScriptedIo::new([
            stopped(1),
            batch(2, BatchOp::Begin, true),
            deleted(3),
            written(4, None),
            written(5, None),
            loaded_ok(6),
            batch(7, BatchOp::Commit, true),
        ]);
        let mut client = LpClient::new(io);
        client
            .replace_and_load_project("demo", &files())
            .await
            .expect("opened");
        assert_eq!(
            ops(&client.into_io().sent),
            [
                "stop",
                "begin",
                "delete demo",
                "write /projects/demo/project.json",
                "write /projects/demo/main.glsl",
                "load projects/demo",
                "commit",
            ]
        );
    }

    /// The same open on a board without: nothing to commit.
    #[tokio::test]
    async fn the_library_open_commits_nothing_on_a_board_without_batches() {
        let io = ScriptedIo::new([
            stopped(1),
            batch(2, BatchOp::Begin, false),
            deleted(3),
            written(4, None),
            written(5, None),
            loaded_ok(6),
        ]);
        let mut client = LpClient::new(io);
        client
            .replace_and_load_project("demo", &files())
            .await
            .expect("opened");
        let sent = ops(&client.into_io().sent);
        assert_eq!(sent.last().map(String::as_str), Some("load projects/demo"));
        assert!(!sent.iter().any(|op| op == "commit" || op == "abort"));
    }

    /// A refused load inside the batch aborts it: the old project is whole.
    #[tokio::test]
    async fn a_refused_open_aborts_its_batch() {
        let io = ScriptedIo::new([
            stopped(1),
            batch(2, BatchOp::Begin, true),
            deleted(3),
            written(4, None),
            written(5, None),
            server_error(6, "a shader did not compile"),
            batch(7, BatchOp::Abort, true),
        ]);
        let mut client = LpClient::new(io);
        let error = client
            .replace_and_load_project("demo", &files())
            .await
            .expect_err("refused");
        assert!(error.to_string().contains("did not compile"), "{error}");
        assert_eq!(
            ops(&client.into_io().sent).last().map(String::as_str),
            Some("abort")
        );
    }

    /// `lp-cli upload`'s deploy: write over (no delete), deflated where that
    /// shrinks a file, one batch committed after the load.
    #[tokio::test]
    async fn a_deploy_writes_over_deflated_in_one_batch() {
        let shader: Vec<u8> = (0..600u32)
            .flat_map(|i| format!("float v{i} = sin({i}.0);\n").into_bytes())
            .collect();
        let chunks = plan_deflated_chunks(&shader, FILE_SYNC_RECORD_MAX_HINT, 10).len();
        let mut script = vec![stopped(1), batch(2, BatchOp::Begin, true), written(3, None)];
        let mut id = 4;
        for _ in 0..chunks {
            script.push(chunk_written(id));
            id += 1;
        }
        script.push(loaded_ok(id));
        script.push(batch(id + 1, BatchOp::Commit, true));
        let mut client = LpClient::new(ScriptedIo::new(script));
        let files = [
            crate::ProjectDeployFile::new("project.json", b"{}".to_vec()),
            crate::ProjectDeployFile::new("main.glsl", shader.clone()),
        ];
        client
            .deploy_project_files("demo", files)
            .await
            .expect("deployed");
        let sent = ops(&client.into_io().sent);
        assert_eq!(
            sent[..3],
            ["stop", "begin", "write /projects/demo/project.json"]
        );
        assert_eq!(sent[3], "deflated /projects/demo/main.glsl @0");
        assert_eq!(sent.len(), 3 + chunks + 2);
        assert_eq!(sent[sent.len() - 2..], ["load projects/demo", "commit"]);
        assert!(
            !sent.iter().any(|op| op.starts_with("delete")),
            "write over"
        );
    }

    fn chunk_written(id: u64) -> lpc_wire::WireServerMessage {
        lpc_wire::WireServerMessage::new(
            id,
            WireServerMsgBody::Filesystem(FsResponse::WriteChunk {
                path: "/projects/demo/main.glsl".as_path_buf(),
                offset: 0,
                written: 0,
                error: None,
            }),
        )
    }

    #[test]
    fn a_small_file_goes_plain() {
        let requests = file_requests("p", "a.json", b"{}", true);
        assert!(matches!(
            requests[..],
            [ClientRequest::Filesystem(FsRequest::Write { .. })]
        ));
    }

    #[test]
    fn a_file_that_does_not_shrink_goes_plain() {
        // Incompressible bytes.
        let mut state = 0x9e37_79b9_u32;
        let noise: Vec<u8> = (0..6000)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect();
        let requests = file_requests("p", "n.bin", &noise, true);
        assert!(
            requests
                .iter()
                .all(|r| matches!(r, ClientRequest::Filesystem(FsRequest::WriteChunk { .. })))
        );
    }

    #[test]
    fn a_text_file_goes_deflated_at_logical_offsets() {
        let text: Vec<u8> = (0..2000u32)
            .flat_map(|i| format!("float x{i} = {i}.0;\n").into_bytes())
            .collect();
        let requests = file_requests("p", "s.glsl", &text, true);
        let mut at = 0u32;
        for request in &requests {
            match request {
                ClientRequest::Filesystem(FsRequest::WriteChunkDeflated {
                    path,
                    offset,
                    logical_len,
                    ..
                }) => {
                    assert_eq!(path.as_str(), "/projects/p/s.glsl");
                    assert_eq!(*offset, at);
                    at += logical_len;
                }
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(at as usize, text.len());
        assert_eq!(
            requests.iter().map(logical_bytes).sum::<u64>(),
            text.len() as u64
        );
        // Without deflate: today's form.
        assert!(
            file_requests("p", "s.glsl", &text, false)
                .iter()
                .all(|r| matches!(r, ClientRequest::Filesystem(FsRequest::WriteChunk { .. })))
        );
    }
}
