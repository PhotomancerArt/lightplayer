use lp_studio_core::{StudioEvent, StudioHeartbeat, StudioLogEntry, StudioLogLevel};
use lpc_wire::server::api::LogLevel;
use lpc_wire::{
    ClientRequest, WireProjectCommand, WireProjectInventoryReadRequest, WireServerMessage,
    WireServerMsgBody,
};

pub fn inventory_request(handle: lpc_wire::WireProjectHandle) -> ClientRequest {
    ClientRequest::ProjectCommand {
        handle,
        command: WireProjectCommand::ReadInventory {
            request: WireProjectInventoryReadRequest,
        },
    }
}

pub fn server_event(response: WireServerMessage) -> Option<StudioEvent> {
    match response.msg {
        WireServerMsgBody::Heartbeat {
            fps,
            frame_count,
            loaded_projects,
            uptime_ms,
            memory,
        } => Some(StudioEvent::HeartbeatReceived {
            heartbeat: StudioHeartbeat {
                fps_avg: fps.avg,
                frame_count,
                loaded_project_count: loaded_projects.len(),
                uptime_ms,
                free_memory_bytes: memory.map(|memory| memory.free_bytes),
            },
        }),
        WireServerMsgBody::Log { level, message } => Some(StudioEvent::LogReceived {
            entry: StudioLogEntry::new(log_level(level), "lp-server", message),
        }),
        _ => None,
    }
}

fn log_level(level: LogLevel) -> StudioLogLevel {
    match level {
        LogLevel::Debug => StudioLogLevel::Debug,
        LogLevel::Info => StudioLogLevel::Info,
        LogLevel::Warn => StudioLogLevel::Warn,
        LogLevel::Error => StudioLogLevel::Error,
    }
}
