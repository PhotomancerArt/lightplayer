//! Browser-owned firmware runtime.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use fw_core::{drain_client_messages, tick_server_frame};
use lpa_server::{
    ButtonService, Graphics, LpGraphics, LpServer, RadioService, RenderTextureRequest,
    VisualProduct,
};
use lpc_hardware::{HardwareSystem, HwRegistry, default_esp32c6_hardware_manifest};
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::time::TimeProvider;
use lpc_wire::{ClientMessage, json};
use lpfs::LpFsMemory;
use lps_shared::TextureStorageFormat;

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
    /// Cached bus-channel → visual-product resolution for the preview path.
    ///
    /// Product handles are node-owned and stable across frames, so the bus is
    /// resolved once per channel and re-resolved only after a render error
    /// (e.g. a project reload invalidated the handle).
    bus_visual_product: Option<(String, VisualProduct)>,
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
        let server = LpServer::new_with_hardware_services(
            output_provider,
            Box::new(LpFsMemory::new()),
            "/projects/".as_path(),
            None,
            Some(time_provider),
            Some(button_service),
            Some(radio_service),
            graphics,
        );

        let mut runtime = Self {
            id,
            label: label.to_string(),
            server,
            transport: BrowserServerTransport::new(),
            time,
            last_tick_ms: 0,
            running: false,
            outbox: Vec::new(),
            bus_visual_product: None,
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

    /// Materialize the visual product on `channel` as sRGB RGBA8 pixels.
    ///
    /// This is the binary preview path: the returned bytes cross the wasm
    /// boundary as a `Uint8Array` and ride `postMessage` as a transferable
    /// buffer, never the JSON envelope path.
    pub(crate) fn render_bus_texture_rgba8(
        &mut self,
        channel: &str,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, String> {
        let handle = self
            .server
            .project_manager()
            .list_loaded_projects()
            .first()
            .map(|project| project.handle)
            .ok_or_else(|| "no project loaded".to_string())?;
        let project = self
            .server
            .project_manager_mut()
            .get_project_mut(handle)
            .ok_or_else(|| format!("loaded project handle {} not found", handle.id()))?;

        let product = match &self.bus_visual_product {
            Some((cached_channel, product)) if cached_channel == channel => *product,
            _ => {
                let product = project
                    .resolve_bus_visual_product(channel)
                    .map_err(|error| format!("{error}"))?;
                self.bus_visual_product = Some((channel.to_string(), product));
                product
            }
        };

        let request = RenderTextureRequest {
            width,
            height,
            format: TextureStorageFormat::Rgba16Unorm,
            time_seconds: project.engine().frame_time().total_ms as f32 / 1000.0,
        };
        let texture = match project.render_visual_texture(product, &request) {
            Ok(texture) => texture,
            Err(error) => {
                // A stale product handle (project reload) renders as an error;
                // drop the cache so the next frame re-resolves the bus.
                self.bus_visual_product = None;
                return Err(format!("{error}"));
            }
        };
        let bytes = texture
            .try_raw_bytes()
            .ok_or_else(|| "render returned non-resident texture".to_string())?;
        Ok(crate::texture_convert::rgba16_unorm_to_rgba8_srgb(bytes))
    }

    fn flush_protocol_out(&mut self) -> Result<(), String> {
        for msg in self.transport.take_outgoing() {
            let frame = json::to_string(&msg)
                .map_err(|error| format!("serialize protocol_out frame: {error}"))?;
            self.outbox.push(BrowserOutputEnvelope::ProtocolOut {
                runtime_id: self.id,
                frame,
            });
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
