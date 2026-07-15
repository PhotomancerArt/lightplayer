//! Browser-owned firmware runtime.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use fw_core::{drain_client_messages, tick_server_frame};
use lpa_server::{ButtonService, Graphics, LpGraphics, LpServer, RadioService};
use lpc_hardware::{HardwareSystem, HwRegistry, default_esp32c6_hardware_manifest};
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::time::TimeProvider;
use lpc_wire::{ClientMessage, json};
use lpfs::LpFsMemory;

use crate::envelope::{BrowserInputEnvelope, BrowserOutputEnvelope};
use crate::executor::block_on;
use crate::manual_time_provider::ManualTimeProvider;
use crate::server_transport::BrowserServerTransport;

/// One in-browser LightPlayer firmware instance.
///
/// The runtime owns the same major pieces as local firmware: server, filesystem,
/// virtual hardware services, output provider, protocol transport, and clock.
pub(crate) struct BrowserFirmwareRuntime {
    id: u32,
    label: String,
    server: LpServer,
    transport: BrowserServerTransport,
    time: ManualTimeProvider,
    last_tick_ms: u64,
    running: bool,
    outbox: Vec<BrowserOutputEnvelope>,
}

impl BrowserFirmwareRuntime {
    /// Build a memory-backed browser firmware runtime.
    pub(crate) fn new(id: u32, label: &str) -> Result<Self, String> {
        let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new_permissive()));
        let hardware = Rc::new(HardwareSystem::with_virtual_drivers(Rc::new(
            HwRegistry::new(default_esp32c6_hardware_manifest()),
        )));
        let button_service: Rc<dyn ButtonService> = hardware.clone();
        let radio_service: Rc<dyn RadioService> = hardware;
        let graphics: Arc<dyn LpGraphics> = Arc::new(Graphics::new());
        let time = ManualTimeProvider::new();
        let time_provider: Rc<dyn TimeProvider> = Rc::new(time.clone());
        let mut server = LpServer::new_with_hardware_services(
            output_provider,
            Box::new(LpFsMemory::new()),
            "/projects/".as_path(),
            None,
            Some(time_provider),
            Some(button_service),
            Some(radio_service),
            graphics,
        );
        // Wire hello payload (sans-IO: injected here). Browser runtimes carry
        // no git provenance or stamped identity; fake devices script a uid in
        // M3.
        server.set_hello(lpc_wire::ServerHello {
            proto: lpc_wire::WIRE_PROTO_VERSION,
            fw: lpc_wire::FwProvenance {
                package: "fw-browser".to_string(),
                commit: "unknown".to_string(),
                dirty: false,
                profile: if cfg!(debug_assertions) {
                    "debug".to_string()
                } else {
                    "release".to_string()
                },
            },
            device_uid: None,
        });

        let mut transport = BrowserServerTransport::new();
        // Wire hello: queued before anything else so it flushes as the first
        // protocol_out frame when the runtime starts serving (the worker
        // drains outputs right after boot). See
        // docs/adr/2026-07-14-wire-hello-versioning.md.
        block_on(fw_core::send_unsolicited_hello(&server, &mut transport))
            .map_err(|error| format!("queue boot hello: {error}"))?;

        let mut runtime = Self {
            id,
            label: label.to_string(),
            server,
            transport,
            time,
            last_tick_ms: 0,
            running: false,
            outbox: Vec::new(),
        };
        runtime.status("booting", Some("browser firmware runtime created"));
        runtime.log("info", "fw-browser runtime booted");
        runtime.status("ready", None);
        Ok(runtime)
    }

    /// Numeric handle used by the wasm runtime registry.
    pub(crate) fn id(&self) -> u32 {
        self.id
    }

    /// Apply one browser input envelope to the runtime.
    pub(crate) fn handle_envelope(&mut self, envelope: BrowserInputEnvelope) -> Result<(), String> {
        match envelope {
            BrowserInputEnvelope::ProtocolIn { frame } => {
                let msg: ClientMessage = json::from_str(&frame)
                    .map_err(|error| format!("parse protocol_in frame: {error}"))?;
                self.transport.push_incoming(msg);
                self.log("debug", "queued protocol_in frame");
                Ok(())
            }
            BrowserInputEnvelope::Tick { delta_ms } => self.tick(delta_ms.unwrap_or(16).max(1)),
            BrowserInputEnvelope::Start => {
                self.running = true;
                self.status("running", None);
                Ok(())
            }
            BrowserInputEnvelope::Stop => {
                self.running = false;
                self.status("stopped", None);
                Ok(())
            }
            BrowserInputEnvelope::Drain => Ok(()),
        }
    }

    /// Advance server time, process queued protocol messages, and tick projects.
    pub(crate) fn tick(&mut self, delta_ms: u32) -> Result<(), String> {
        self.time.advance(delta_ms);
        let frame_start_ms = self.time.now_ms();
        let drained = block_on(drain_client_messages(&mut self.transport));
        if let Some(error) = &drained.error {
            self.log("warn", &format!("transport receive error: {error}"));
        }
        let incoming_count = drained.message_count();
        let tick = block_on(tick_server_frame(
            &mut self.server,
            &mut self.transport,
            &self.time,
            frame_start_ms,
            self.last_tick_ms,
            drained.messages,
        ));
        self.last_tick_ms = frame_start_ms;
        if let Some(error) = tick.server_error {
            self.status("error", Some(&format!("server tick error: {error}")));
        }

        self.log(
            "trace",
            &format!(
                "tick delta={}ms incoming={} responses={} frame={}us",
                tick.delta_ms, incoming_count, tick.response_count, tick.frame_time_us
            ),
        );
        self.flush_protocol_out()?;
        Ok(())
    }

    /// Serialize and clear all queued runtime output envelopes.
    pub(crate) fn drain_output_json(&mut self) -> Result<String, String> {
        self.flush_protocol_out()?;
        let messages = core::mem::take(&mut self.outbox);
        serde_json::to_string(&messages).map_err(|error| format!("serialize envelopes: {error}"))
    }

    fn flush_protocol_out(&mut self) -> Result<(), String> {
        for msg in self.transport.take_outgoing() {
            let frame = json::to_string(&msg)
                .map_err(|error| format!("serialize protocol_out frame: {error}"))?;
            self.outbox
                .push(BrowserOutputEnvelope::ProtocolOut { frame });
        }
        Ok(())
    }

    fn status(&mut self, status: &str, message: Option<&str>) {
        self.outbox.push(BrowserOutputEnvelope::Status {
            runtime_id: self.id,
            status: status.to_string(),
            message: message.map(str::to_string),
        });
    }

    fn log(&mut self, level: &str, message: &str) {
        self.outbox.push(BrowserOutputEnvelope::Log {
            runtime_id: self.id,
            level: level.to_string(),
            target: "fw-browser".to_string(),
            message: format!("{}: {message}", self.label),
        });
    }
}
