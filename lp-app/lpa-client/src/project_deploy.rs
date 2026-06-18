//! Helpers for planning project uploads over the server protocol.
//!
//! This module owns the common stop/write/load request order that Studio, CLI,
//! and future agents should share when they deploy a project through a running
//! `lp-server`.

use lpc_model::AsLpPathBuf;
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

    fn into_write_request(self, project_id: &str) -> ClientRequest {
        ClientRequest::Filesystem(FsRequest::Write {
            path: project_file_path(project_id, &self.relative_path).as_path_buf(),
            data: self.bytes,
        })
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

/// Build write requests without changing project lifecycle.
pub fn project_write_requests(
    project_id: &str,
    files: impl IntoIterator<Item = ProjectDeployFile>,
) -> Vec<ClientRequest> {
    files
        .into_iter()
        .map(|file| file.into_write_request(project_id))
        .collect()
}

/// Build the current deploy flow: stop loaded projects, write files, load.
pub fn project_deploy_requests(
    project_id: &str,
    files: impl IntoIterator<Item = ProjectDeployFile>,
) -> Vec<ClientRequest> {
    let mut requests = Vec::new();
    requests.push(ClientRequest::StopAllProjects);
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
        ClientRequest::Filesystem(FsRequest::Read { .. }) => "fs.read",
        ClientRequest::Filesystem(FsRequest::Write { .. }) => "fs.write",
        ClientRequest::Filesystem(FsRequest::DeleteFile { .. }) => "fs.delete_file",
        ClientRequest::Filesystem(FsRequest::DeleteDir { .. }) => "fs.delete_dir",
        ClientRequest::Filesystem(FsRequest::ListDir { .. }) => "fs.list_dir",
        ClientRequest::LoadProject { .. } => "project.load",
        ClientRequest::UnloadProject { .. } => "project.unload",
        ClientRequest::ProjectRequest { .. } => "project.read",
        ClientRequest::ProjectCommand { .. } => "project.command",
        ClientRequest::ListAvailableProjects => "project.list_available",
        ClientRequest::ListLoadedProjects => "project.list_loaded",
        ClientRequest::StopAllProjects => "project.stop_all",
    }
}

fn normalize_relative_path(path: &str) -> String {
    path.trim_start_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deploy_requests_stop_write_then_load() {
        let requests = project_deploy_requests(
            "demo",
            [
                ProjectDeployFile::new("project.toml", b"project".to_vec()),
                ProjectDeployFile::new("/shader.glsl", b"shader".to_vec()),
            ],
        );

        assert!(matches!(requests[0], ClientRequest::StopAllProjects));
        assert!(matches!(
            &requests[1],
            ClientRequest::Filesystem(FsRequest::Write { path, .. })
                if path.as_str() == "/projects/demo/project.toml"
        ));
        assert!(matches!(
            &requests[2],
            ClientRequest::Filesystem(FsRequest::Write { path, .. })
                if path.as_str() == "/projects/demo/shader.glsl"
        ));
        assert!(matches!(
            &requests[3],
            ClientRequest::LoadProject { path } if path == "projects/demo"
        ));
    }
}
