//! Message handlers for LpServer

extern crate alloc;

use crate::error::ServerError;
use crate::project_manager::ProjectManager;
use alloc::{format, rc::Rc, vec::Vec};
use core::cell::RefCell;
use lp_model::{
    AsLpPath, ClientMessage, LpPath, LpPathBuf, ServerMessage,
    server::{AvailableProject, FsRequest, FsResponse, ServerMsgBody as ServerMessagePayload},
};
use lp_shared::fs::LpFs;
use lp_shared::output::OutputProvider;

/// Handle a client message and generate a server response
pub fn handle_client_message(
    project_manager: &mut ProjectManager,
    base_fs: &mut dyn LpFs,
    output_provider: &Rc<RefCell<dyn OutputProvider>>,
    client_msg: ClientMessage,
    theoretical_fps: Option<f32>,
) -> Result<ServerMessage, ServerError> {
    let ClientMessage { id, msg } = client_msg;

    let response = match msg {
        lp_model::ClientRequest::Filesystem(fs_request) => {
            ServerMessagePayload::Filesystem(handle_fs_request(base_fs, fs_request)?)
        }
        lp_model::ClientRequest::LoadProject { path } => {
            handle_load_project(project_manager, base_fs, output_provider, path.as_path())?
        }
        lp_model::ClientRequest::UnloadProject { handle } => {
            handle_unload_project(project_manager, handle)?
        }
        lp_model::ClientRequest::ProjectRequest { handle, request } => {
            handle_project_request(project_manager, handle, request, theoretical_fps)?
        }
        lp_model::ClientRequest::ListAvailableProjects => {
            handle_list_available_projects(project_manager, base_fs)?
        }
        lp_model::ClientRequest::ListLoadedProjects => {
            handle_list_loaded_projects(project_manager)?
        }
        lp_model::ClientRequest::StopAllProjects => handle_stop_all_projects(project_manager)?,
    };

    Ok(ServerMessage { id, msg: response })
}

/// Handle a filesystem request
fn handle_fs_request(fs: &mut dyn LpFs, request: FsRequest) -> Result<FsResponse, ServerError> {
    match request {
        FsRequest::Read { path } => match fs.read_file(path.as_path()) {
            Ok(data) => Ok(FsResponse::Read {
                path,
                data: Some(data),
                error: None,
            }),
            Err(e) => Ok(FsResponse::Read {
                path,
                data: None,
                error: Some(format!("{e}")),
            }),
        },
        FsRequest::Write { path, data } => match fs.write_file(path.as_path(), &data) {
            Ok(()) => Ok(FsResponse::Write { path, error: None }),
            Err(e) => Ok(FsResponse::Write {
                path,
                error: Some(format!("{e}")),
            }),
        },
        FsRequest::DeleteFile { path } => match fs.delete_file(path.as_path()) {
            Ok(()) => Ok(FsResponse::DeleteFile { path, error: None }),
            Err(e) => Ok(FsResponse::DeleteFile {
                path,
                error: Some(format!("{e}")),
            }),
        },
        FsRequest::DeleteDir { path } => match fs.delete_dir(path.as_path()) {
            Ok(()) => Ok(FsResponse::DeleteDir { path, error: None }),
            Err(e) => Ok(FsResponse::DeleteDir {
                path,
                error: Some(format!("{e}")),
            }),
        },
        FsRequest::ListDir { path, recursive } => match fs.list_dir(path.as_path(), recursive) {
            Ok(entries) => Ok(FsResponse::ListDir {
                path,
                entries,
                error: None,
            }),
            Err(e) => Ok(FsResponse::ListDir {
                path,
                entries: Vec::new(),
                error: Some(format!("{e}")),
            }),
        },
    }
}

/// Handle a LoadProject request
fn handle_load_project(
    project_manager: &mut ProjectManager,
    base_fs: &mut dyn LpFs,
    output_provider: &Rc<RefCell<dyn OutputProvider>>,
    path: &LpPath,
) -> Result<ServerMessagePayload, ServerError> {
    let handle = project_manager.load_project(path, base_fs, output_provider.clone())?;
    Ok(ServerMessagePayload::LoadProject { handle })
}

/// Handle an UnloadProject request
fn handle_unload_project(
    project_manager: &mut ProjectManager,
    handle: lp_model::project::ProjectHandle,
) -> Result<ServerMessagePayload, ServerError> {
    project_manager.unload_project(handle)?;
    Ok(ServerMessagePayload::UnloadProject)
}

/// Handle a ProjectRequest (project-specific request)
fn handle_project_request(
    project_manager: &mut ProjectManager,
    handle: lp_model::project::ProjectHandle,
    request: lp_model::project::api::ProjectRequest,
    theoretical_fps: Option<f32>,
) -> Result<ServerMessagePayload, ServerError> {
    let project = project_manager
        .get_project_mut(handle)
        .ok_or_else(|| ServerError::ProjectNotFound(format!("handle {}", handle.id())))?;

    match request {
        lp_model::project::api::ProjectRequest::GetChanges {
            since_frame,
            detail_specifier,
        } => {
            let current_frame_before = project.runtime_mut().frame_id;
            log::debug!(
                "handle_project_request: GetChanges request (since_frame: {}, current_frame: {})",
                since_frame.as_i64(),
                current_frame_before.as_i64()
            );
            let response = project
                .runtime_mut()
                .get_changes(since_frame, &detail_specifier, theoretical_fps)
                .map_err(|e| ServerError::Core(format!("Failed to get changes: {e}")))?;

            let response_frame = match &response {
                lp_model::project::api::ProjectResponse::GetChanges { current_frame, .. } => {
                    *current_frame
                }
            };
            log::debug!(
                "handle_project_request: GetChanges response (current_frame: {})",
                response_frame.as_i64()
            );

            let serializable_response = response
                .to_serializable()
                .map_err(|e| ServerError::Core(format!("Failed to serialize response: {e}")))?;

            Ok(ServerMessagePayload::ProjectRequest {
                response: serializable_response,
            })
        }
    }
}

/// Handle a ListAvailableProjects request
fn handle_list_available_projects(
    project_manager: &ProjectManager,
    base_fs: &dyn LpFs,
) -> Result<ServerMessagePayload, ServerError> {
    let names = project_manager.list_available_projects(base_fs)?;
    let projects = names
        .into_iter()
        .map(|name| {
            // Build full path
            let base_dir = LpPathBuf::from(project_manager.projects_base_dir());
            let path = base_dir.join(&name);
            AvailableProject { path }
        })
        .collect();
    Ok(ServerMessagePayload::ListAvailableProjects { projects })
}

/// Handle a ListLoadedProjects request
fn handle_list_loaded_projects(
    project_manager: &ProjectManager,
) -> Result<ServerMessagePayload, ServerError> {
    let projects = project_manager.list_loaded_projects();
    Ok(ServerMessagePayload::ListLoadedProjects { projects })
}

/// Handle a StopAllProjects request
fn handle_stop_all_projects(
    project_manager: &mut ProjectManager,
) -> Result<ServerMessagePayload, ServerError> {
    project_manager.unload_all_projects();
    Ok(ServerMessagePayload::StopAllProjects)
}
