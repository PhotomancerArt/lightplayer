//! Helpers for planning project uploads over the server protocol.
//!
//! The conversation itself is [`crate::push_files::deploy_files`]; this
//! module holds the paths, the file type, and the request list a board
//! without transactions receives from a deploy (stop, begin — answered
//! `atomic: false` — writes, load), for tests that drive a server with it.

use lpc_wire::{ClientRequest, FsRequest, WireProjectHandle, WireServerMsgBody};

use crate::client_error::{ClientError, ClientResult};

/// One file to write under `/projects/{project_id}`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectDeployFile {
    relative_path: String,
    bytes: Vec<u8>,
}

impl ProjectDeployFile {
    pub fn new(relative_path: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            relative_path: normalize_relative_path(&relative_path.into()),
            bytes: bytes.into(),
        }
    }

    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

pub fn project_load_path(project_id: &str) -> String {
    format!("projects/{project_id}")
}

/// Build the absolute server filesystem path for a project file.
pub fn project_file_path(project_id: &str, relative_path: &str) -> String {
    format!(
        "/projects/{project_id}/{}",
        normalize_relative_path(relative_path)
    )
}

/// The write requests a push sends for `files`, without changing project
/// lifecycle: deflated where that shrinks a file, else `Write` /
/// `WriteChunk` runs ([`crate::push_files::file_requests`]).
pub fn project_write_requests(
    project_id: &str,
    files: impl IntoIterator<Item = ProjectDeployFile>,
) -> Vec<ClientRequest> {
    files
        .into_iter()
        .flat_map(|file| {
            crate::push_files::file_requests(project_id, &file.relative_path, &file.bytes, true)
        })
        .collect()
}

/// What `deploy_project_files` sends a board without transactions: stop
/// loaded projects, begin a batch (answered `atomic: false`, so nothing is
/// opened), write the files, load. A board with transactions also gets a
/// `CommitBatch` after the load.
pub fn project_deploy_requests(
    project_id: &str,
    files: impl IntoIterator<Item = ProjectDeployFile>,
) -> Vec<ClientRequest> {
    let mut requests = Vec::new();
    requests.push(ClientRequest::StopAllProjects);
    requests.push(ClientRequest::Filesystem(FsRequest::BeginBatch));
    requests.extend(project_write_requests(project_id, files));
    requests.push(ClientRequest::LoadProject {
        path: project_load_path(project_id),
    });
    requests
}

/// Validate one deploy response and return the loaded project handle if present.
pub fn validate_project_deploy_response(
    request: &ClientRequest,
    response: &WireServerMsgBody,
) -> ClientResult<Option<WireProjectHandle>> {
    match (request, response) {
        (ClientRequest::StopAllProjects, WireServerMsgBody::StopAllProjects) => Ok(None),
        (
            ClientRequest::Filesystem(
                FsRequest::BeginBatch | FsRequest::CommitBatch | FsRequest::AbortBatch,
            ),
            WireServerMsgBody::Filesystem(lpc_wire::FsResponse::Batch { error, .. }),
        ) => match error {
            Some(error) => Err(ClientError::Server(format!("batch: {error}"))),
            None => Ok(None),
        },
        (
            ClientRequest::Filesystem(FsRequest::Write { path, .. }),
            WireServerMsgBody::Filesystem(lpc_wire::FsResponse::Write { error, .. }),
        ) => {
            if let Some(error) = error {
                Err(ClientError::Server(format!(
                    "failed to write {}: {error}",
                    path.as_str()
                )))
            } else {
                Ok(None)
            }
        }
        (
            ClientRequest::Filesystem(
                FsRequest::WriteChunk { path, .. } | FsRequest::WriteChunkDeflated { path, .. },
            ),
            WireServerMsgBody::Filesystem(lpc_wire::FsResponse::WriteChunk { error, .. }),
        ) => {
            if let Some(error) = error {
                Err(ClientError::Server(format!(
                    "failed to write chunk of {}: {error}",
                    path.as_str()
                )))
            } else {
                Ok(None)
            }
        }
        (ClientRequest::LoadProject { .. }, WireServerMsgBody::LoadProject { handle }) => {
            Ok(Some(*handle))
        }
        _ => Err(ClientError::unexpected_response(
            request_label(request),
            response,
        )),
    }
}

pub fn request_label(request: &ClientRequest) -> &'static str {
    match request {
        ClientRequest::Hello => "hello",
        ClientRequest::Filesystem(FsRequest::Read { .. }) => "fs.read",
        ClientRequest::Filesystem(FsRequest::Write { .. }) => "fs.write",
        ClientRequest::Filesystem(FsRequest::DeleteFile { .. }) => "fs.delete_file",
        ClientRequest::Filesystem(FsRequest::DeleteDir { .. }) => "fs.delete_dir",
        ClientRequest::Filesystem(FsRequest::ListDir { .. }) => "fs.list_dir",
        ClientRequest::Filesystem(FsRequest::ChangesSince { .. }) => "fs.changes_since",
        ClientRequest::Filesystem(FsRequest::WriteChunk { .. }) => "fs.write_chunk",
        ClientRequest::Filesystem(FsRequest::HashPackage { .. }) => "fs.hash_package",
        ClientRequest::Filesystem(FsRequest::WriteChunkDeflated { .. }) => {
            "fs.write_chunk_deflated"
        }
        ClientRequest::Filesystem(FsRequest::BeginBatch) => "fs.begin_batch",
        ClientRequest::Filesystem(FsRequest::CommitBatch) => "fs.commit_batch",
        ClientRequest::Filesystem(FsRequest::AbortBatch) => "fs.abort_batch",
        ClientRequest::LoadProject { .. } => "project.load",
        ClientRequest::UnloadProject { .. } => "project.unload",
        ClientRequest::ProjectRead { .. } => "project.read",
        ClientRequest::ProjectCommand { .. } => "project.command",
        ClientRequest::ListAvailableProjects => "project.list_available",
        ClientRequest::ListLoadedProjects => "project.list_loaded",
        ClientRequest::StopAllProjects => "project.stop_all",
        ClientRequest::SetLogLevel { .. } => "server.set_log_level",
        ClientRequest::Reboot => "server.reboot",
        ClientRequest::ClearFaults => "server.clear_faults",
        ClientRequest::SetEncoding { .. } => "server.set_encoding",
        ClientRequest::LoginBegin => "access.login_begin",
        ClientRequest::LoginAnswer { .. } => "access.login_answer",
        ClientRequest::AccessList => "access.list",
        ClientRequest::AccessAdd { .. } => "access.add",
        ClientRequest::AccessRemove { .. } => "access.remove",
        ClientRequest::AccessSetSwitches { .. } => "access.set_switches",
        ClientRequest::NetworkStatus => "wifi.status",
        ClientRequest::NetworkScan => "wifi.scan",
        ClientRequest::NetworkAdd { .. } => "wifi.add",
        ClientRequest::NetworkForget { .. } => "wifi.forget",
        ClientRequest::NetworkSet { .. } => "wifi.set",
    }
}

fn normalize_relative_path(path: &str) -> String {
    path.trim_start_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deploy_requests_stop_begin_write_then_load() {
        let requests = project_deploy_requests(
            "demo",
            [
                ProjectDeployFile::new("project.toml", b"project".to_vec()),
                ProjectDeployFile::new("/shader.glsl", b"shader".to_vec()),
            ],
        );

        assert!(matches!(requests[0], ClientRequest::StopAllProjects));
        assert!(matches!(
            requests[1],
            ClientRequest::Filesystem(FsRequest::BeginBatch)
        ));
        assert!(matches!(
            &requests[2],
            ClientRequest::Filesystem(FsRequest::Write { path, .. })
                if path.as_str() == "/projects/demo/project.toml"
        ));
        assert!(matches!(
            &requests[3],
            ClientRequest::Filesystem(FsRequest::Write { path, .. })
                if path.as_str() == "/projects/demo/shader.glsl"
        ));
        assert!(matches!(
            &requests[4],
            ClientRequest::LoadProject { path } if path == "projects/demo"
        ));
    }
}
