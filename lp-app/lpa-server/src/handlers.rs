//! Message handlers for LpServer

extern crate alloc;

use crate::error::ServerError;
use crate::project_manager::ProjectManager;
use crate::server::MemoryStatsFn;
use alloc::{format, rc::Rc, sync::Arc, vec::Vec};
use core::cell::RefCell;
use lpc_engine::{ButtonService, LpGraphics, RadioService};
use lpc_model::{AsLpPath, LpPath, LpPathBuf};
use lpc_shared::backtrace;
use lpc_shared::output::OutputProvider;
use lpc_shared::time::TimeProvider;
use lpc_wire::{
    WireServerMessage, WireServerMsgBody as ServerMessagePayload,
    messages::ClientMessage,
    server::{AvailableProject, FsRequest, FsResponse},
};
use lpfs::LpFs;

/// Log memory stats if callback is provided and returns values
fn log_memory(memory_stats: Option<&MemoryStatsFn>, label: &str) {
    if let Some(f) = memory_stats {
        if let Some((free, used)) = f() {
            log::info!(
                "[mem] {}: {}k free / {}k used",
                label,
                free / 1024,
                used / 1024
            );
        }
    }
}

/// Handle a client message and generate a server response
pub fn handle_client_message(
    project_manager: &mut ProjectManager,
    base_fs: &mut dyn LpFs,
    output_provider: &Rc<RefCell<dyn OutputProvider>>,
    memory_stats: Option<&MemoryStatsFn>,
    time_provider: Option<Rc<dyn TimeProvider>>,
    button_service: Option<Rc<dyn ButtonService>>,
    radio_service: Option<Rc<dyn RadioService>>,
    graphics: Arc<dyn LpGraphics>,
    client_msg: ClientMessage,
) -> Result<WireServerMessage, ServerError> {
    let ClientMessage { id, msg } = client_msg;

    let response = match msg {
        lpc_wire::ClientRequest::Filesystem(fs_request) => {
            ServerMessagePayload::Filesystem(handle_fs_request(base_fs, fs_request)?)
        }
        lpc_wire::ClientRequest::LoadProject { path } => handle_load_project(
            project_manager,
            base_fs,
            output_provider,
            memory_stats,
            time_provider,
            button_service,
            radio_service,
            graphics,
            path.as_path(),
        )?,
        lpc_wire::ClientRequest::UnloadProject { handle } => {
            handle_unload_project(project_manager, memory_stats, handle)?
        }
        lpc_wire::ClientRequest::ProjectRequest { .. } => {
            return Err(ServerError::Core(
                "ProjectRequest must be handled by streaming transport".into(),
            ));
        }
        lpc_wire::ClientRequest::ListAvailableProjects => {
            handle_list_available_projects(project_manager, base_fs)?
        }
        lpc_wire::ClientRequest::ListLoadedProjects => {
            handle_list_loaded_projects(project_manager)?
        }
        lpc_wire::ClientRequest::StopAllProjects => {
            handle_stop_all_projects(project_manager, memory_stats)?
        }
    };

    Ok(WireServerMessage { id, msg: response })
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
    memory_stats: Option<&MemoryStatsFn>,
    time_provider: Option<Rc<dyn TimeProvider>>,
    button_service: Option<Rc<dyn ButtonService>>,
    radio_service: Option<Rc<dyn RadioService>>,
    graphics: Arc<dyn LpGraphics>,
    path: &LpPath,
) -> Result<ServerMessagePayload, ServerError> {
    backtrace::set_oom_context("server handler: load project");
    log::info!("Loading project: {}", path.as_str());
    log_memory(memory_stats, "load_project before");
    let handle = project_manager.load_project(
        path,
        base_fs,
        output_provider.clone(),
        memory_stats.copied(),
        time_provider,
        button_service,
        radio_service,
        graphics,
    )?;
    backtrace::set_oom_context("server handler: load project memory log");
    log_memory(memory_stats, "load_project after");
    backtrace::set_oom_context("server handler: load project response");
    let response = ServerMessagePayload::LoadProject { handle };
    backtrace::clear_oom_context();
    Ok(response)
}

/// Handle an UnloadProject request
fn handle_unload_project(
    project_manager: &mut ProjectManager,
    _memory_stats: Option<&MemoryStatsFn>,
    handle: lpc_wire::WireProjectHandle,
) -> Result<ServerMessagePayload, ServerError> {
    log::info!("Unloading project handle {}", handle.id());
    project_manager.unload_project(handle)?;
    Ok(ServerMessagePayload::UnloadProject)
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
    memory_stats: Option<&MemoryStatsFn>,
) -> Result<ServerMessagePayload, ServerError> {
    let count = project_manager.list_loaded_projects().len();
    log::info!("Stopping all projects ({count} loaded)");
    log_memory(memory_stats, "stop_all_projects before");
    project_manager.unload_all_projects()?;
    log_memory(memory_stats, "stop_all_projects after");
    log::info!("Stopped all projects");
    Ok(ServerMessagePayload::StopAllProjects)
}
