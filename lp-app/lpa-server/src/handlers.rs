//! Message handlers for LpServer

extern crate alloc;

use crate::error::ServerError;
use crate::project_manager::ProjectManager;
use crate::server::{MemoryStatsFn, ReadHeadroomProbe, check_load_headroom};
use alloc::{format, rc::Rc, sync::Arc, vec::Vec};
use core::cell::RefCell;
use lpc_engine::{ButtonService, LpGraphics, RadioService};
use lpc_model::{AsLpPath, LpPath, LpPathBuf};
use lpc_shared::backtrace;
use lpc_shared::output::OutputProvider;
use lpc_shared::time::TimeProvider;
use lpc_wire::{
    WireProjectCommand, WireProjectCommandResponse, WireServerMessage,
    WireServerMsgBody as ServerMessagePayload,
    messages::ClientMessage,
    server::{AvailableProject, FsRequest, FsResponse},
};
use lpfs::LpFs;

/// Log memory stats if callback is provided and returns values
fn log_memory(memory_stats: Option<&MemoryStatsFn>, label: &str) {
    if let Some(f) = memory_stats {
        if let Some((free, used)) = f() {
            // Bytes as well as KB, deliberately. The `load_project` and
            // `stop_all_projects` before/after pairs are the cheapest bracket
            // we have on what a project costs, and the classic ESP32's per-LED
            // figure — which its advertised LED ceiling is derived from — comes
            // from differencing two such brackets. At KB granularity that
            // difference carries ±8 B/LED of rounding, enough to hide a whole
            // optimisation. See `docs/adr/2026-08-01-esp32v3-flash-budget.md`.
            log::info!(
                "[mem] {}: {} B free / {} B used ({}k / {}k)",
                label,
                free,
                used,
                free / 1024,
                used / 1024
            );
        }
    }
}

/// Handle a client message and generate a server response
/// Device/link state a freshly loaded engine must wear
/// (`LpServer::load_project` applies the same pair on the host-call path).
#[derive(Clone, Copy, Debug)]
pub struct EngineLinkState {
    /// Engine-ready display-layout byte budget (`None` = unbounded link).
    pub display_layout_budget: Option<usize>,
    /// Device-level safe-mode output ceiling.
    pub safe_output_clamp: Option<u8>,
}

impl Default for EngineLinkState {
    /// The fail-safe posture, matching a fresh engine's own default: the
    /// serial frame budget and no clamp. An un-plumbed caller refuses big
    /// layouts rather than wedging a serial link; unbounded links opt out
    /// explicitly (`LpServer::set_project_read_frame_budget(None)`).
    fn default() -> Self {
        Self {
            display_layout_budget: Some(
                lpc_wire::PROJECT_READ_FRAME_MAX_BYTES
                    - lpc_wire::PROJECT_READ_PROBE_HEADER_RESERVE_BYTES,
            ),
            safe_output_clamp: None,
        }
    }
}

pub fn handle_client_message(
    project_manager: &mut ProjectManager,
    base_fs: &mut dyn LpFs,
    output_provider: &Rc<RefCell<dyn OutputProvider>>,
    memory_stats: Option<&MemoryStatsFn>,
    read_headroom_probe: Option<ReadHeadroomProbe>,
    // Whether the embedder installed a reset action
    // (`LpServer::set_reboot_hook`). Deliberately the FACT and not the hook
    // itself: this function decides whether `Reboot` can be honored, while
    // firing the hook belongs to the caller, once the ack is on the wire.
    reboot_supported: bool,
    time_provider: Option<Rc<dyn TimeProvider>>,
    button_service: Option<Rc<dyn ButtonService>>,
    radio_service: Option<Rc<dyn RadioService>>,
    graphics: Arc<dyn LpGraphics>,
    hello: &lpc_wire::ServerHello,
    link_state: EngineLinkState,
    client_msg: ClientMessage,
) -> Result<WireServerMessage, ServerError> {
    let ClientMessage { id, msg } = client_msg;

    let response = match msg {
        // A FIXTURE build refuses the hello outright, which is what
        // pre-hello firmware looks like from the client's side: the
        // request goes unanswered and absence IS the mismatch signal
        // (`docs/adr/2026-07-14-wire-hello-versioning.md`). Suppressing
        // only the unsolicited hello would not do it — Studio's client
        // asks as well, and an answer would make the board compatible
        // again. Never enable this in a released image.
        #[cfg(feature = "fixture-no-hello")]
        lpc_wire::ClientRequest::Hello => {
            return Err(ServerError::Core(alloc::string::String::from(
                "hello is not supported",
            )));
        }
        #[cfg(not(feature = "fixture-no-hello"))]
        lpc_wire::ClientRequest::Hello => {
            // The injected hello's `device_uid` is a boot-time hint; the
            // root identity file is live truth (stamping happens at
            // runtime), so answer requests with a fresh read.
            let mut hello = hello.clone();
            hello.device_uid = crate::device_identity::read_device_uid(&*base_fs);
            ServerMessagePayload::Hello(hello)
        }
        lpc_wire::ClientRequest::Filesystem(fs_request) => {
            ServerMessagePayload::Filesystem(handle_fs_request(base_fs, fs_request)?)
        }
        lpc_wire::ClientRequest::LoadProject { path } => handle_load_project(
            project_manager,
            base_fs,
            output_provider,
            memory_stats,
            read_headroom_probe,
            time_provider,
            button_service,
            radio_service,
            graphics,
            link_state,
            path.as_path(),
        )?,
        lpc_wire::ClientRequest::UnloadProject { handle } => {
            handle_unload_project(project_manager, memory_stats, handle)?
        }
        lpc_wire::ClientRequest::ProjectRead { .. } => {
            return Err(ServerError::Core(
                "project reads must be handled by streaming transport".into(),
            ));
        }
        lpc_wire::ClientRequest::ProjectCommand { handle, command } => {
            ServerMessagePayload::ProjectCommand {
                response: handle_project_command(project_manager, handle, command)?,
            }
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
        lpc_wire::ClientRequest::SetLogLevel { level } => handle_set_log_level(level),
        lpc_wire::ClientRequest::Reboot => {
            if !reboot_supported {
                return Err(ServerError::Core(alloc::string::String::from(
                    "reboot is not supported by this server: no reset action is wired",
                )));
            }
            // The ack only; the reset itself happens after this frame is
            // written (`LpServer::tick_and_send`) — see `RebootHook`.
            ServerMessagePayload::Reboot
        }
        lpc_wire::ClientRequest::ClearFaults => {
            // Both halves, in this order for no reason but readability:
            // the ledger stops denying the frames, and the engine re-arms
            // the nodes that gave up. Neither alone is a retry — a re-armed
            // node whose path is still gated faults again on the next tick.
            //
            // No reboot hook equivalent: nothing resets and nothing is
            // retried here. The next tick does the work, and if the failure
            // is still there the node faults again and the heartbeat says
            // so. Answering "cleared" is not a promise that it is fixed.
            let ledger_cleared = lp_recovery::clear_ledger();
            project_manager.clear_faults();
            ServerMessagePayload::ClearFaults { ledger_cleared }
        }
    };

    Ok(WireServerMessage::new(id, response))
}

fn handle_project_command(
    project_manager: &mut ProjectManager,
    handle: lpc_wire::WireProjectHandle,
    command: WireProjectCommand,
) -> Result<WireProjectCommandResponse, ServerError> {
    let project = project_manager
        .get_project_mut(handle)
        .ok_or_else(|| ServerError::ProjectNotFound(format!("handle {}", handle.id())))?;

    match command {
        WireProjectCommand::ReadOverlay { request: _ } => {
            Ok(WireProjectCommandResponse::ReadOverlay {
                response: project.read_overlay(),
            })
        }
        WireProjectCommand::MutateOverlay { request } => {
            Ok(WireProjectCommandResponse::MutateOverlay {
                response: project.mutate_overlay(request)?,
            })
        }
        WireProjectCommand::CommitOverlay { request } => {
            Ok(WireProjectCommandResponse::CommitOverlay {
                response: project.commit_overlay(request)?,
            })
        }
        WireProjectCommand::ReadInventory { request: _ } => {
            Ok(WireProjectCommandResponse::ReadInventory {
                response: project.read_inventory(),
            })
        }
        WireProjectCommand::CreateNode { request } => Ok(WireProjectCommandResponse::CreateNode {
            response: project.create_node(request)?,
        }),
        WireProjectCommand::RemoveNode { request } => Ok(WireProjectCommandResponse::RemoveNode {
            response: project.remove_node(request)?,
        }),
        WireProjectCommand::NodeCommand { node, command } => {
            Ok(WireProjectCommandResponse::NodeCommand {
                response: project.node_command(node, &command),
            })
        }
        WireProjectCommand::PanelWrite { request } => Ok(WireProjectCommandResponse::PanelWrite {
            response: project.panel_write(&request),
        }),
        WireProjectCommand::PanelClear { request } => Ok(WireProjectCommandResponse::PanelClear {
            response: project.panel_clear(&request),
        }),
        WireProjectCommand::PanelAutoSave { request } => {
            Ok(WireProjectCommandResponse::PanelAutoSave {
                response: project.panel_auto_save_command(&request),
            })
        }
    }
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
        FsRequest::Write { path, data } => {
            // Dispatch marker for the flash-write-wedge diagnosis: pairs
            // with fw-esp32v3's [FLASH] traces to split "request never
            // dispatched" from "storage op wedged" from "response lost".
            // docs/defects/2026-08-29-flash-write-wedges-under-zook-playback.md
            log::debug!("fs write dispatch: {} ({} B)", path.as_str(), data.len());
            let result = fs.write_file(path.as_path(), &data);
            log::debug!("fs write handled: {} ok={}", path.as_str(), result.is_ok());
            match result {
                Ok(()) => Ok(FsResponse::Write { path, error: None }),
                Err(e) => Ok(FsResponse::Write {
                    path,
                    error: Some(format!("{e}")),
                }),
            }
        }
        FsRequest::DeleteFile { path } => match fs.delete_file(path.as_path()) {
            Ok(()) => Ok(FsResponse::DeleteFile { path, error: None }),
            Err(e) => Ok(FsResponse::DeleteFile {
                path,
                error: Some(format!("{e}")),
            }),
        },
        // deleting an absent dir succeeds: the goal state already holds
        // (LittleFS surfaces missing dirs as generic Filesystem errors,
        // so probe existence rather than matching error kinds)
        FsRequest::DeleteDir { path } if !fs.is_dir(path.as_path()).unwrap_or(false) => {
            Ok(FsResponse::DeleteDir { path, error: None })
        }
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
        FsRequest::ChangesSince {
            prefix,
            since,
            cursor,
        } => Ok(crate::file_sync::handle_changes_since(
            fs,
            prefix.as_path(),
            since,
            cursor,
        )),
        FsRequest::WriteChunk { path, offset, data } => Ok(crate::file_sync::handle_write_chunk(
            fs, path, offset, &data,
        )),
        FsRequest::HashPackage { prefix } => Ok(crate::file_sync::handle_hash_package(fs, prefix)),
    }
}

/// Handle a LoadProject request
fn handle_load_project(
    project_manager: &mut ProjectManager,
    base_fs: &mut dyn LpFs,
    output_provider: &Rc<RefCell<dyn OutputProvider>>,
    memory_stats: Option<&MemoryStatsFn>,
    read_headroom_probe: Option<ReadHeadroomProbe>,
    time_provider: Option<Rc<dyn TimeProvider>>,
    button_service: Option<Rc<dyn ButtonService>>,
    radio_service: Option<Rc<dyn RadioService>>,
    graphics: Arc<dyn LpGraphics>,
    link_state: EngineLinkState,
    path: &LpPath,
) -> Result<ServerMessagePayload, ServerError> {
    backtrace::set_oom_context("server handler: load project");
    log::info!("Loading project: {}", path.as_str());
    let loaded_count = project_manager.list_loaded_projects().len();
    if loaded_count > 0 {
        log::info!(
            "Unloading {loaded_count} project(s) before loading {}",
            path.as_str()
        );
        log_memory(memory_stats, "load_project unload existing before");
        project_manager.unload_all_projects()?;
        log_memory(memory_stats, "load_project unload existing after");
    }
    // Gated AFTER the unload on purpose: the probe must read the heap the
    // load would actually run in, and refusing before freeing the outgoing
    // project would reject loads that fit.
    check_load_headroom(read_headroom_probe)?;
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
    // The clamp and the display-layout budget are device/link state: every
    // engine wears them, including one born from a wire-load. Skipping this
    // left wire-loaded projects on the fail-safe SERIAL budget, so an
    // unbounded link (the browser sim) silently refused dome-scale display
    // layouts — the lamp preview drew only the fixtures that fit.
    if let Some(project) = project_manager.get_project_mut(handle) {
        project
            .engine_mut()
            .set_safe_output_clamp(link_state.safe_output_clamp);
        project
            .engine_mut()
            .set_display_layout_budget(link_state.display_layout_budget);
    }
    backtrace::set_oom_context("server handler: load project memory log");
    log_memory(memory_stats, "load_project after");
    persist_startup_project(base_fs, path);
    backtrace::set_oom_context("server handler: load project response");
    let response = ServerMessagePayload::LoadProject { handle };
    backtrace::clear_oom_context();
    Ok(response)
}

/// Remember the loaded project as the boot default: a device that
/// power-cycles resumes the last project it was told to show. Best-effort —
/// a config write failure must never fail the load itself.
fn persist_startup_project(fs: &dyn LpFs, path: &LpPath) {
    use alloc::string::ToString;
    use lpc_model::server::server_config::ServerConfig;

    let Some(name) = path
        .as_str()
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
    else {
        return;
    };

    let mut config = fs
        .read_file(ServerConfig::PATH.as_path())
        .ok()
        .and_then(|data| lpc_wire::json::from_slice::<ServerConfig>(&data).ok())
        .unwrap_or_default();
    if config.startup_project.as_deref() == Some(name) {
        return;
    }
    config.startup_project = Some(name.to_string());

    match lpc_wire::json::to_string(&config) {
        Ok(json) => {
            if let Err(error) = fs.write_file(ServerConfig::PATH.as_path(), json.as_bytes()) {
                log::warn!("load_project: failed to persist startup_project: {error}");
            } else {
                log::info!("load_project: startup_project set to {name}");
            }
        }
        Err(error) => {
            log::warn!("load_project: failed to serialize server config: {error}");
        }
    }
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
    // With faults: this answer is the OTHER thing the device card's
    // running face is made of (the model asks it right after a push, not
    // waiting a heartbeat period), so it must not be the honest heartbeat's
    // amnesiac twin.
    let projects = project_manager.list_loaded_projects_with_faults();
    Ok(ServerMessagePayload::ListLoadedProjects { projects })
}

/// Handle a SetLogLevel request.
///
/// Applies the level process-globally via [`log::set_max_level`] — every
/// platform that serves the protocol (ESP32, emulator, browser worker, host)
/// routes its diagnostics through the `log` crate, so this one call is the
/// runtime lever on all of them. Nothing is persisted: a device reboot
/// reverts to the logger-init default (Info).
fn handle_set_log_level(level: lpc_wire::server::api::LogLevel) -> ServerMessagePayload {
    // Log before applying so the confirmation is visible under the *old*
    // level even when the new level would suppress Info output.
    log::info!("setting log level to {level:?}");
    log::set_max_level(log_level_filter(level));
    ServerMessagePayload::SetLogLevel
}

/// Wire [`lpc_wire::server::api::LogLevel`] → [`log::LevelFilter`].
///
/// Total by construction: the wire enum deliberately has no `Off`, so the
/// client can never turn the device fully silent.
fn log_level_filter(level: lpc_wire::server::api::LogLevel) -> log::LevelFilter {
    use lpc_wire::server::api::LogLevel;
    match level {
        LogLevel::Trace => log::LevelFilter::Trace,
        LogLevel::Debug => log::LevelFilter::Debug,
        LogLevel::Info => log::LevelFilter::Info,
        LogLevel::Warn => log::LevelFilter::Warn,
        LogLevel::Error => log::LevelFilter::Error,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Deleting an absent dir reports success: the goal state already
    /// holds. Regression for push-onto-a-fresh-device failing with the
    /// device fs's "no such file or directory" during the replace clear.
    #[test]
    fn delete_dir_on_a_missing_dir_succeeds() {
        use lpc_model::AsLpPathBuf;
        let mut fs = lpfs::LpFsMemory::new();

        let response = handle_fs_request(
            &mut fs,
            FsRequest::DeleteDir {
                path: "/projects/studio".as_path_buf(),
            },
        )
        .expect("handler runs");

        assert!(
            matches!(response, FsResponse::DeleteDir { error: None, .. }),
            "absent dir deletes as a no-op, got {response:?}"
        );
    }

    /// `log::set_max_level` is process-global, so this single test exercises
    /// several levels and restores the original value at the end, keeping it
    /// robust under parallel test execution.
    #[test]
    fn set_log_level_applies_globally_and_acks() {
        let original = log::max_level();

        let ack = handle_set_log_level(lpc_wire::server::api::LogLevel::Trace);
        assert!(matches!(ack, ServerMessagePayload::SetLogLevel));
        assert_eq!(log::max_level(), log::LevelFilter::Trace);

        handle_set_log_level(lpc_wire::server::api::LogLevel::Error);
        assert_eq!(log::max_level(), log::LevelFilter::Error);

        log::set_max_level(original);
    }
}
