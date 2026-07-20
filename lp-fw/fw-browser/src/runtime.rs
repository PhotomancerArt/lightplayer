//! Browser-owned firmware runtime.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use fw_core::{drain_client_messages, tick_server_frame};
use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{
    ButtonService, LpGraphics, LpServer, RadioService, RenderTextureRequest, TextureRenderProduct,
    VisualProduct,
};
use lpc_hardware::{HardwareSystem, HwRegistry, default_esp32c6_hardware_manifest};
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::time::TimeProvider;
use lpc_wire::{ClientMessage, json};
use lpfs::LpFsMemory;
use lps_shared::TextureStorageFormat;

use lp_gfx_wgpu::GpuGraphics;

use crate::envelope::{BrowserInputEnvelope, BrowserOutputEnvelope};
use crate::executor::block_on;
use crate::gpu::{self, WorkerGpu};
use crate::manual_time_provider::ManualTimeProvider;
use crate::preview_surface::PreviewSurface;
use crate::server_transport::BrowserServerTransport;
use crate::tier::{RuntimeTier, TierSelection};

/// GLSL frontend for the browser runtime's CPU tier.
///
/// Naga: the browser CPU tier has compiled through naga since the GPU tier
/// landed — kept as the explicit choice rather than an accident of feature
/// unification. The `lp-shader/naga` frontend it needs is enabled explicitly
/// via this crate's `lpa-server` dependency (`features = ["naga"]`); the
/// naga *crate* that `lp-gfx-wgpu` compiles for the GPU tier does not enable
/// it. The device constant is [`lpa_server::DEVICE_SHADER_FRONTEND`]
/// (LpsGlsl); converging the browser tier onto it is a product decision to
/// make deliberately, not here.
const BROWSER_SHADER_FRONTEND: lpa_server::ShaderFrontend = lpa_server::ShaderFrontend::Naga;

// The sidecar builds this crate standalone (`cargo build -p fw-browser`), so
// no workspace feature unification supplies the frontend: if the `naga`
// feature chain breaks, every shader compile fails at runtime and Studio's
// simulator renders black. Fail the build instead.
const _: () = assert!(
    BROWSER_SHADER_FRONTEND.built_in(),
    "the pinned shader frontend is not compiled in; keep `features = [\"naga\"]` on fw-browser's lpa-server dependency"
);

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
    /// The tier chosen at creation (fidelity-tiers ADR: recorded, surfaced,
    /// never silently changed).
    tier: TierSelection,
    /// GPU-tier presentation state (`None` on the CPU tier).
    gpu: Option<GpuRuntimeState>,
}

/// GPU-tier pieces owned by one runtime: the worker's shared device, the
/// runtime's `GpuGraphics` backend, and (once attached) its card surface.
///
/// Dropping this (runtime destruction) is clean even while a canvas is
/// attached: wgpu's WebGPU backend `Surface` drop is an explicit no-op that
/// just releases its JS refs (`GPUCanvasContext` + canvas), and the
/// `OffscreenCanvas` was transferred into the worker, so the surface holds
/// the only reference and JS GC reclaims both. The worker's shared device
/// outlives the runtime via `worker_gpu`.
struct GpuRuntimeState {
    worker_gpu: Rc<WorkerGpu>,
    graphics: Arc<GpuGraphics>,
    surface: Option<PreviewSurface>,
}

impl BrowserFirmwareRuntime {
    /// Build a memory-backed browser firmware runtime on the requested tier.
    ///
    /// Tier selection happens here, exactly once: a `Gpu` request while the
    /// worker device is unavailable produces a CPU-tier runtime with the
    /// reason recorded (visible state, not an error — the CPU tier is always
    /// functional). The selection is emitted as one structured log line and
    /// returned to the worker script for the `runtime_created` message.
    pub(crate) fn new(id: u32, label: &str, requested: RuntimeTier) -> Result<Self, String> {
        let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new_permissive()));
        let hardware = Rc::new(HardwareSystem::with_virtual_drivers(Rc::new(
            HwRegistry::new(default_esp32c6_hardware_manifest()),
        )));
        let button_service: Rc<dyn ButtonService> = hardware.clone();
        let radio_service: Rc<dyn RadioService> = hardware;

        let (tier, gpu) = match requested {
            RuntimeTier::Cpu => (TierSelection::granted(RuntimeTier::Cpu), None),
            RuntimeTier::Gpu => match gpu::device() {
                Ok(worker_gpu) => {
                    let graphics = Arc::new(GpuGraphics::new(
                        worker_gpu.device.clone(),
                        worker_gpu.queue.clone(),
                        Box::new(TargetLpvmGraphics::new(BROWSER_SHADER_FRONTEND)),
                    ));
                    (
                        TierSelection::granted(RuntimeTier::Gpu),
                        Some(GpuRuntimeState {
                            worker_gpu,
                            graphics,
                            surface: None,
                        }),
                    )
                }
                Err(reason) => (TierSelection::cpu_because(reason), None),
            },
        };
        let graphics: Arc<dyn LpGraphics> = match &gpu {
            Some(state) => state.graphics.clone(),
            None => Arc::new(TargetLpvmGraphics::new(BROWSER_SHADER_FRONTEND)),
        };
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
            bus_visual_product: None,
            tier,
            gpu,
        };
        runtime.status("booting", Some("browser firmware runtime created"));
        runtime.log("info", "fw-browser runtime booted");
        // The one structured tier-selection log line (fidelity-tiers ADR).
        let tier_line = match (&runtime.tier.tier, &runtime.tier.reason) {
            (RuntimeTier::Gpu, _) => "tier selected: tier=gpu backend=wgpu".to_string(),
            (RuntimeTier::Cpu, None) => "tier selected: tier=cpu backend=lpvm".to_string(),
            (RuntimeTier::Cpu, Some(reason)) => {
                format!("tier selected: tier=cpu backend=lpvm requested=gpu reason={reason}")
            }
        };
        runtime.log("info", &tier_line);
        runtime.status("ready", None);
        Ok(runtime)
    }

    /// Numeric handle used by the wasm runtime registry.
    pub(crate) fn id(&self) -> u32 {
        self.id
    }

    /// The tier recorded at creation.
    pub(crate) fn tier(&self) -> &TierSelection {
        &self.tier
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
        let texture = self.render_bus_texture(channel, width, height)?;
        let bytes = texture.try_raw_bytes().ok_or_else(|| {
            "render produced a GPU-resident texture (GPU-tier runtime); use surface \
             presentation (`present_frame`) instead of the byte transport"
                .to_string()
        })?;
        Ok(crate::texture_convert::rgba16_unorm_to_rgba8_srgb(bytes))
    }

    /// Attach a transferred `OffscreenCanvas` as this runtime's card surface.
    ///
    /// GPU tier only: the CPU tier presents through the `preview_frame` byte
    /// transport and never gets a surface (explicit answer, no fallback).
    pub(crate) fn attach_preview_surface(
        &mut self,
        canvas: web_sys::OffscreenCanvas,
    ) -> Result<(), String> {
        let id = self.id;
        let tier_reason = self.tier.reason.clone();
        let Some(gpu) = self.gpu.as_mut() else {
            let reason = tier_reason
                .map(|reason| format!(" (gpu unavailable: {reason})"))
                .unwrap_or_default();
            return Err(format!(
                "runtime {id} is on the CPU tier{reason}; surface presentation requires the \
                 GPU tier"
            ));
        };
        gpu.surface = Some(PreviewSurface::attach(&gpu.worker_gpu, canvas)?);
        self.log("info", "preview surface attached");
        Ok(())
    }

    /// Render the visual product on `channel` and present it to the attached
    /// card surface (zero readback — the GPU-tier presentation path).
    pub(crate) fn present_bus_texture(&mut self, channel: &str) -> Result<(), String> {
        // Device-lost is a terminal, visible state: every present fails with
        // the recorded reason and the card shows the error (no retry loop).
        if let Some(lost) = gpu::device_lost() {
            return Err(format!("gpu device lost: {lost}"));
        }
        let (width, height) = {
            let gpu = self
                .gpu
                .as_ref()
                .ok_or_else(|| "present requires a GPU-tier runtime".to_string())?;
            let surface = gpu
                .surface
                .as_ref()
                .ok_or_else(|| "present before attach_surface".to_string())?;
            (surface.width(), surface.height())
        };

        let texture = self.render_bus_texture(channel, width, height)?;
        let gpu = self.gpu.as_ref().expect("gpu state checked above");
        let surface = gpu.surface.as_ref().expect("surface checked above");

        // Host-resident products (e.g. the fluid node's CPU-sim texels) are
        // legal on the GPU tier: bridge them with a one-frame upload. Only a
        // product with neither a handle nor host bytes is a bug.
        let uploaded;
        let handle = match texture.gpu_handle() {
            Some(handle) => handle,
            None => {
                let bytes = texture.try_raw_bytes().ok_or_else(|| {
                    "render produced a texture with neither GPU handle nor host bytes".to_string()
                })?;
                uploaded = gpu
                    .graphics
                    .create_texture(
                        texture.width(),
                        texture.height(),
                        texture.storage_format(),
                        bytes,
                    )
                    .map_err(|error| format!("upload host product for present: {error}"))?;
                &uploaded
            }
        };

        gpu.graphics
            .present_to_surface(handle, surface.surface())
            .map_err(|error| format!("present to surface: {error}"))
    }

    /// Resolve (with caching) and render the visual product on a bus channel.
    fn render_bus_texture(
        &mut self,
        channel: &str,
        width: u32,
        height: u32,
    ) -> Result<TextureRenderProduct, String> {
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
        match project.render_visual_texture(product, &request) {
            Ok(texture) => Ok(texture),
            Err(error) => {
                // A stale product handle (project reload) renders as an error;
                // drop the cache so the next frame re-resolves the bus.
                self.bus_visual_product = None;
                Err(format!("{error}"))
            }
        }
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
