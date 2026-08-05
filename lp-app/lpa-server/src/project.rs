//! Project wrapper for managing a single project instance

extern crate alloc;

use crate::error::ServerError;
use crate::panel_state::{self, PANEL_STATE_WRITE_INTERVAL_MS};
use crate::server::MemoryStatsFn;
use alloc::{boxed::Box, format, rc::Rc, string::String, sync::Arc, vec::Vec};
use core::cell::RefCell;
use lpc_engine::{ButtonService, Engine, EngineServices, LpGraphics, ProjectLoader, RadioService};
use lpc_hardware::HwEndpointSpec;
use lpc_model::{LpPath, LpPathBuf, TreePath, current_revision};
use lpc_registry::{ParseCtx, ProjectRegistry};
use lpc_shared::backtrace;
use lpc_shared::output::{OutputChannelHandle, OutputDriverOptions, OutputFormat, OutputProvider};
use lpc_shared::time::TimeProvider;
use lpc_wire::{
    WireCreateNodeRequest, WireCreateNodeResponse, WireNodeCommand, WireNodeCommandResponse,
    WireOverlayCommitRequest, WireOverlayCommitResponse, WireOverlayMutationRequest,
    WireOverlayMutationResponse, WireOverlayReadResponse, WireProjectInventoryReadResponse,
    WireRemoveNodeRequest, WireRemoveNodeResponse,
};
use lpfs::{FsEvent, FsVersion, LpFs};

/// A project instance wrapping one loaded engine.
pub struct Project {
    /// Project name/identifier
    name: String,
    /// Project filesystem path
    path: LpPathBuf,
    /// Chrooted filesystem for this project.
    fs: Rc<RefCell<dyn LpFs>>,
    /// Shared output provider used by engine services and manual recovery reloads.
    output_provider: Rc<RefCell<dyn OutputProvider>>,
    /// Shared time provider used by engine services and manual recovery reloads.
    time_provider: Option<Rc<dyn TimeProvider>>,
    /// Shared button service used by engine services and manual recovery reloads.
    button_service: Option<Rc<dyn ButtonService>>,
    /// Shared radio service used by engine services and manual recovery reloads.
    radio_service: Option<Rc<dyn RadioService>>,
    /// Optional memory stats callback for project load/reload checkpoints.
    memory_stats: Option<MemoryStatsFn>,
    /// Graphics backend used by shader runtime nodes.
    graphics: Arc<dyn LpGraphics>,
    /// Canonical project registry: artifacts, overlay, effective defs/assets.
    registry: ProjectRegistry,
    /// The loaded project engine.
    runtime: Option<Engine>,
    /// Last filesystem version processed by this project
    last_fs_version: FsVersion,
    /// How many fs-event batches actually reached
    /// `apply_project_changes`. Framework-tier writes (`/.lp/**`) are
    /// filtered out before that seam, so this is the diagnostic that says
    /// whether a write cost a binding-graph rebuild.
    applied_refreshes: u64,
    /// Whether panel state keeps saving (panel.md P11 — on by default,
    /// restored from the state file itself).
    panel_auto_save: bool,
    /// Engine-time since the last panel-state write, for the flash
    /// preservation throttle.
    panel_state_age_ms: u32,
    /// Writer-store mutation count as of the last write, so an idle
    /// project writes nothing however long it runs.
    panel_state_saved_mutations: u64,
}

impl Project {
    /// Create a new project instance
    ///
    /// The project must already exist on the filesystem.
    /// Takes an OutputProvider from the server as Rc<RefCell> (for no_std compatibility).
    pub fn new(
        name: String,
        path: &LpPath,
        fs: Rc<RefCell<dyn LpFs>>,
        output_provider: Rc<RefCell<dyn OutputProvider>>,
        memory_stats: Option<MemoryStatsFn>,
        time_provider: Option<Rc<dyn TimeProvider>>,
        button_service: Option<Rc<dyn ButtonService>>,
        radio_service: Option<Rc<dyn RadioService>>,
        graphics: Arc<dyn LpGraphics>,
        loaded_fs_version: FsVersion,
    ) -> Result<Self, ServerError> {
        log_memory(memory_stats, "project new start");
        backtrace::set_oom_context("project new: root path");
        let root_path = project_root_path(&name)?;
        log_memory(memory_stats, "project new after root path");
        backtrace::set_oom_context("project new: engine services");
        let services = build_engine_services(
            root_path,
            output_provider.clone(),
            time_provider.clone(),
            button_service.clone(),
            radio_service.clone(),
        );
        log_memory(memory_stats, "project new after services");

        backtrace::set_oom_context("project new: load core project");
        let (mut runtime, registry) = {
            let fs_ref = fs.borrow();
            ProjectLoader::load_from_root(&*fs_ref, services)
                .map_err(|e| ServerError::Core(format!("Failed to load core project: {e}")))?
                .into_parts()
        };
        log_memory(memory_stats, "project new after core project");
        backtrace::set_oom_context("project new: set graphics");
        runtime.set_graphics(Some(graphics.clone()));
        log_memory(memory_stats, "project new after graphics");

        // Panel state comes back BEFORE the first tick, therefore before
        // the first render (panel.md P10): the scarf that was dimmed from
        // a phone must not flash bright on replug, not for one frame.
        // This is the boot seam on device too — `auto_load_project` runs
        // here, ahead of the main loop.
        backtrace::set_oom_context("project new: restore panel state");
        let panel_auto_save = {
            let fs_ref = fs.borrow();
            panel_state::restore(&*fs_ref, &mut runtime)
        };

        backtrace::set_oom_context("project new: build wrapper");
        let panel_state_saved_mutations = runtime.panel_writers().mutations();
        let project = Self {
            name,
            path: path.to_path_buf(),
            fs,
            output_provider,
            time_provider,
            button_service,
            radio_service,
            memory_stats,
            graphics,
            registry,
            runtime: Some(runtime),
            last_fs_version: loaded_fs_version.next(),
            applied_refreshes: 0,
            panel_auto_save,
            panel_state_age_ms: 0,
            panel_state_saved_mutations,
        };
        log_memory(memory_stats, "project new after wrapper");
        backtrace::clear_oom_context();
        Ok(project)
    }

    /// Get the project name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the project path
    pub fn path(&self) -> &LpPath {
        &self.path
    }

    /// Get mutable access to the loaded engine.
    pub fn engine_mut(&mut self) -> &mut Engine {
        self.runtime
            .as_mut()
            .expect("project runtime is only absent while reloading")
    }

    /// Get immutable access to the loaded engine.
    pub fn engine(&self) -> &Engine {
        self.runtime
            .as_ref()
            .expect("project runtime is only absent while reloading")
    }

    pub fn registry(&self) -> &ProjectRegistry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut ProjectRegistry {
        &mut self.registry
    }

    /// Split borrow for read paths that walk the engine against the
    /// registry (probes need `&mut Engine` for the resolver while reading
    /// registry state). Public so host-level tests can drive probes.
    pub fn runtime_read_parts(&mut self) -> (&mut Engine, &ProjectRegistry) {
        let runtime = self
            .runtime
            .as_mut()
            .expect("project runtime is only absent while reloading");
        (runtime, &self.registry)
    }

    pub fn tick(&mut self, delta_ms: u32) -> Result<(), ServerError> {
        let registry = &self.registry;
        let runtime = self
            .runtime
            .as_mut()
            .expect("project runtime is only absent while reloading");
        let result = runtime
            .tick(registry, delta_ms)
            .map_err(|e| ServerError::Core(format!("{e}")));
        // Persistence rides the tick, throttled — a failed frame still
        // gets its panel state written, since a crash loop is exactly
        // when losing the user's dim would hurt most.
        self.persist_panel_state_if_due(delta_ms);
        result
    }

    /// Write panel state if it changed and the throttle window has
    /// elapsed (panel.md P11: flash preservation — a knob wiggled for a
    /// minute writes ~6 times, not once per input event).
    fn persist_panel_state_if_due(&mut self, delta_ms: u32) {
        self.panel_state_age_ms = self.panel_state_age_ms.saturating_add(delta_ms);
        if !self.panel_auto_save || self.panel_state_age_ms < PANEL_STATE_WRITE_INTERVAL_MS {
            return;
        }
        if self.panel_state_mutations() == self.panel_state_saved_mutations {
            // Nothing engaged, cleared, or moved since the last write:
            // an idle project never touches flash however long it runs.
            self.panel_state_age_ms = 0;
            return;
        }
        self.write_panel_state();
    }

    /// Write panel state now, regardless of the throttle — the
    /// clean-shutdown flush (panel.md P11), so the last few seconds of a
    /// gesture are not lost on an orderly stop.
    pub fn flush_panel_state(&mut self) {
        if !self.panel_auto_save || self.panel_state_mutations() == self.panel_state_saved_mutations
        {
            return;
        }
        self.write_panel_state();
    }

    fn write_panel_state(&mut self) {
        let file = {
            let runtime = self
                .runtime
                .as_ref()
                .expect("project runtime is only absent while reloading");
            panel_state::snapshot(runtime, self.panel_auto_save)
        };
        let fs_ref = self.fs.borrow();
        panel_state::write(&*fs_ref, &file);
        self.panel_state_saved_mutations = self.panel_state_mutations();
        self.panel_state_age_ms = 0;
    }

    fn panel_state_mutations(&self) -> u64 {
        self.runtime
            .as_ref()
            .map(|runtime| runtime.panel_writers().mutations())
            .unwrap_or(self.panel_state_saved_mutations)
    }

    /// Whether panel state is being saved (panel.md P11 — on by default).
    pub fn panel_auto_save(&self) -> bool {
        self.panel_auto_save
    }

    /// Turn panel-state saving on or off. The choice is itself persisted,
    /// so it survives a reboot; turning it OFF rewrites the file once so
    /// the next boot knows, and turning it on flushes current state.
    pub fn set_panel_auto_save(&mut self, auto_save: bool) {
        if self.panel_auto_save == auto_save {
            return;
        }
        self.panel_auto_save = auto_save;
        let file = {
            let runtime = self
                .runtime
                .as_ref()
                .expect("project runtime is only absent while reloading");
            panel_state::snapshot(runtime, auto_save)
        };
        let fs_ref = self.fs.borrow();
        panel_state::write(&*fs_ref, &file);
        drop(fs_ref);
        self.panel_state_saved_mutations = self.panel_state_mutations();
        self.panel_state_age_ms = 0;
    }

    /// Resolve the visual product handle currently carried by a bus channel.
    ///
    /// Preview surfaces call this once after load (product handles are stable
    /// across frames) and then materialize frames with
    /// [`Self::render_visual_texture`].
    pub fn resolve_bus_visual_product(
        &mut self,
        channel: &str,
    ) -> Result<lpc_engine::products::visual::VisualProduct, ServerError> {
        let (engine, registry) = self.runtime_read_parts();
        engine
            .resolve_bus_visual_product(registry, channel)
            .map_err(|e| ServerError::Core(format!("resolve bus visual product: {e}")))
    }

    /// Materialize a visual product into a CPU texture (preview path).
    pub fn render_visual_texture(
        &mut self,
        product: lpc_engine::products::visual::VisualProduct,
        request: &lpc_engine::products::visual::RenderTextureRequest,
    ) -> Result<lpc_engine::products::visual::TextureRenderProduct, ServerError> {
        let (engine, registry) = self.runtime_read_parts();
        engine
            .render_texture_product(registry, product, request)
            .map_err(|e| ServerError::Core(format!("render visual texture: {e}")))
    }

    pub fn read_overlay(&mut self) -> WireOverlayReadResponse {
        // Base-value display strings ride the read as a parallel list (P2):
        // one base parse per overlaid artifact annotates every pending path,
        // so reconnecting clients and foreign-edit fetches restore "old
        // value" displays without extra requests.
        let shapes = self.engine().slot_shapes().clone();
        let ctx = ParseCtx { shapes: &shapes };
        let base_values = {
            let fs_ref = self.fs.borrow();
            self.registry.overlay_base_displays(&*fs_ref, &ctx)
        };
        let overlay = self.registry.overlay();
        WireOverlayReadResponse::new(overlay.get().clone(), overlay.changed_at())
            .with_base_values(base_values)
    }

    pub fn read_inventory(&self) -> WireProjectInventoryReadResponse {
        let index = self.engine().project_runtime_index();
        WireProjectInventoryReadResponse::from_inventory_with_runtime_ids(
            self.registry.inventory(),
            |use_location| index.node_id(use_location),
        )
    }

    /// Dispatch a runtime node command to the engine.
    ///
    /// Rejections (unknown node, dead runtime, unsupported command,
    /// out-of-range payload) are NORMAL responses, never a request-envelope
    /// error: a stale click must not poison the connection or any node's
    /// runtime status.
    pub fn node_command(
        &mut self,
        node: lpc_model::NodeId,
        command: &WireNodeCommand,
    ) -> WireNodeCommandResponse {
        match self.engine_mut().handle_node_command(node, command) {
            Ok(()) => WireNodeCommandResponse::Accepted,
            Err(error) => WireNodeCommandResponse::Rejected {
                reason: format!("{error}"),
            },
        }
    }

    /// Dispatch a panel write (panel.md P8): engage/update the writer at
    /// `(scope, channel)`. Runtime state only — no overlay, no dirty.
    pub fn panel_write(
        &mut self,
        request: &lpc_wire::WirePanelWriteRequest,
    ) -> lpc_wire::WirePanelCommandResponse {
        let scope = lpc_engine::node::ScopeRef::from_wire(request.scope);
        // A write to a scope no node introduces is a stale gesture from a
        // client racing an edit — reject normally, never poison.
        let engine = self.engine_mut();
        if engine.tree().get(scope.owner()).is_none() {
            return lpc_wire::WirePanelCommandResponse::Rejected {
                reason: alloc::format!("unknown scope owner {:?}", scope.owner()),
            };
        }
        engine.panel_write(
            scope,
            lpc_model::ChannelName(request.channel.clone()),
            request.value.clone(),
            request.ttl_ms,
        );
        lpc_wire::WirePanelCommandResponse::Accepted {
            engaged: engine.panel_writers().len() as u32,
        }
    }

    /// Dispatch a panel clear (panel.md P3; P-Q4: `All` reaches sink
    /// scopes too).
    pub fn panel_clear(
        &mut self,
        request: &lpc_wire::WirePanelClearRequest,
    ) -> lpc_wire::WirePanelCommandResponse {
        let engine = self.engine_mut();
        match request {
            lpc_wire::WirePanelClearRequest::Channel { scope, channel } => {
                let scope = lpc_engine::node::ScopeRef::from_wire(*scope);
                engine.panel_clear(scope, &lpc_model::ChannelName(channel.clone()));
            }
            lpc_wire::WirePanelClearRequest::Scope { scope } => {
                let scope = lpc_engine::node::ScopeRef::from_wire(*scope);
                engine.panel_clear_scope(scope);
            }
            lpc_wire::WirePanelClearRequest::All => {
                engine.panel_clear_all();
            }
        }
        lpc_wire::WirePanelCommandResponse::Accepted {
            engaged: engine.panel_writers().len() as u32,
        }
    }

    /// Dispatch the panel-state auto-save toggle (panel.md P11). Always
    /// accepted: the flag is project-level state with no scope to be stale
    /// about, and [`Self::set_panel_auto_save`] is idempotent. The
    /// engaged-writer count rides back so the toggle answers in exactly
    /// the shape the other panel commands do.
    pub fn panel_auto_save_command(
        &mut self,
        request: &lpc_wire::WirePanelAutoSaveRequest,
    ) -> lpc_wire::WirePanelCommandResponse {
        self.set_panel_auto_save(request.enabled);
        lpc_wire::WirePanelCommandResponse::Accepted {
            engaged: self.engine().panel_writers().len() as u32,
        }
    }

    pub fn mutate_overlay(
        &mut self,
        request: WireOverlayMutationRequest,
    ) -> Result<WireOverlayMutationResponse, ServerError> {
        let frame = current_revision();
        let shapes = self.engine().slot_shapes().clone();
        let ctx = ParseCtx { shapes: &shapes };
        let result = {
            let fs_ref = self.fs.borrow();
            self.registry
                .mutate_batch(&*fs_ref, request.batch, frame, &ctx)
        };
        {
            let fs_ref = self.fs.borrow();
            self.runtime
                .as_mut()
                .expect("project runtime is only absent while reloading")
                .apply_project_changes(&*fs_ref, &mut self.registry, &result.changes)
                .map_err(|e| ServerError::Core(format!("apply project changes: {e}")))?;
        }
        Ok(WireOverlayMutationResponse::new(
            result.commands,
            result.overlay_revision,
        ))
    }

    /// Create and attach a node (commit-immediate; never staged in the
    /// overlay). An accepted create drives the same
    /// [`Engine::apply_project_changes`] path `mutate_overlay` uses, so the
    /// new node runs immediately; a rejection changes nothing and rides the
    /// response as data.
    pub fn create_node(
        &mut self,
        request: WireCreateNodeRequest,
    ) -> Result<WireCreateNodeResponse, ServerError> {
        let frame = current_revision();
        let shapes = self.engine().slot_shapes().clone();
        let ctx = ParseCtx { shapes: &shapes };
        let result = {
            let fs_ref = self.fs.borrow();
            self.registry.create_node(
                &*fs_ref,
                request.file.as_path(),
                &request.body,
                &request.assets,
                &request.attach,
                frame,
                &ctx,
            )
        };
        match result {
            Ok(outcome) => {
                let written_fs_version = {
                    let fs_ref = self.fs.borrow();
                    self.runtime
                        .as_mut()
                        .expect("project runtime is only absent while reloading")
                        .apply_project_changes(&*fs_ref, &mut self.registry, &outcome.changes)
                        .map_err(|e| ServerError::Core(format!("apply project changes: {e}")))?;
                    fs_ref.current_version()
                };
                // The registry already refreshed from its own writes; skip
                // them in the fs-watcher refresh loop (same as commit).
                self.last_fs_version = written_fs_version.next();
                Ok(WireCreateNodeResponse::Created {
                    artifact_changes: outcome.artifact_changes,
                    revision: frame,
                })
            }
            Err(rejection) => Ok(WireCreateNodeResponse::Rejected { rejection }),
        }
    }

    /// Stage a node removal in the overlay (revertible until commit). An
    /// accepted removal drives the same [`Engine::apply_project_changes`]
    /// path `mutate_overlay` uses, so the node's runtime subtree tears down
    /// immediately (`uses.removed`); the staged deletes materialize on the
    /// next overlay commit. A rejection changes nothing and rides the
    /// response as data.
    pub fn remove_node(
        &mut self,
        request: WireRemoveNodeRequest,
    ) -> Result<WireRemoveNodeResponse, ServerError> {
        let frame = current_revision();
        let shapes = self.engine().slot_shapes().clone();
        let ctx = ParseCtx { shapes: &shapes };
        let result = {
            let fs_ref = self.fs.borrow();
            self.registry
                .remove_node(&*fs_ref, &request.site, frame, &ctx)
        };
        match result {
            Ok(outcome) => {
                {
                    let fs_ref = self.fs.borrow();
                    self.runtime
                        .as_mut()
                        .expect("project runtime is only absent while reloading")
                        .apply_project_changes(&*fs_ref, &mut self.registry, &outcome.changes)
                        .map_err(|e| ServerError::Core(format!("apply project changes: {e}")))?;
                }
                Ok(WireRemoveNodeResponse::Staged {
                    overlay_revision: self.registry.overlay().changed_at(),
                    staged_deletes: outcome.staged_deletes,
                    swept_pending_edits: outcome.swept_pending_edits,
                })
            }
            Err(rejection) => Ok(WireRemoveNodeResponse::Rejected { rejection }),
        }
    }

    pub fn commit_overlay(
        &mut self,
        _request: WireOverlayCommitRequest,
    ) -> Result<WireOverlayCommitResponse, ServerError> {
        let frame = current_revision();
        let shapes = self.engine().slot_shapes().clone();
        let ctx = ParseCtx { shapes: &shapes };
        let (result, committed_fs_version) = {
            let fs_ref = self.fs.borrow();
            let result = self
                .registry
                .commit_overlay(&*fs_ref, frame, &ctx)
                .map_err(|e| ServerError::Core(format!("commit overlay: {e:?}")))?;
            (result, fs_ref.current_version())
        };
        self.last_fs_version = committed_fs_version.next();
        Ok(WireOverlayCommitResponse::new(
            result,
            self.registry.overlay().changed_at(),
        ))
    }

    pub fn refresh_artifacts(&mut self, events: &[FsEvent]) -> Result<(), ServerError> {
        // The framework tier is not authored content. Panel state
        // (`/.lp/panel.json`) is written from inside the tick, and the
        // write fires an FsEvent right back at us — without this filter
        // every ~10s save would clear and re-register the whole binding
        // graph, and a knob left engaged would rebuild the project
        // forever. Filter FIRST, before anything reads the batch.
        let events: Vec<FsEvent> = events
            .iter()
            .filter(|event| is_project_artifact_path(&event.path))
            .cloned()
            .collect();
        if events.is_empty() {
            return Ok(());
        }
        self.applied_refreshes = self.applied_refreshes.saturating_add(1);
        let events = events.as_slice();
        let frame = current_revision();
        let shapes = self.engine().slot_shapes().clone();
        let ctx = ParseCtx { shapes: &shapes };
        let changes = {
            let fs_ref = self.fs.borrow();
            self.registry
                .refresh_artifacts(&*fs_ref, events, frame, &ctx)
        };
        {
            let fs_ref = self.fs.borrow();
            self.runtime
                .as_mut()
                .expect("project runtime is only absent while reloading")
                .apply_project_changes(&*fs_ref, &mut self.registry, &changes)
                .map_err(|e| ServerError::Core(format!("apply project changes: {e}")))?;
        }
        Ok(())
    }

    /// Manually reload the registry and runtime from durable artifacts.
    ///
    /// Normal overlay mutation and filesystem refresh paths use incremental
    /// registry-driven apply. This is a recovery path for callers that want to
    /// discard live runtime state and rebuild from the committed filesystem.
    pub fn reload(&mut self) -> Result<(), ServerError> {
        log_memory(self.memory_stats, "project reload start");
        backtrace::set_oom_context("project reload: drop old runtime");
        drop(self.runtime.take());
        log_memory(self.memory_stats, "project reload after drop old runtime");
        backtrace::set_oom_context("project reload: root path");
        let root_path = project_root_path(&self.name)?;
        log_memory(self.memory_stats, "project reload after root path");
        backtrace::set_oom_context("project reload: engine services");
        let services = build_engine_services(
            root_path,
            self.output_provider.clone(),
            self.time_provider.clone(),
            self.button_service.clone(),
            self.radio_service.clone(),
        );
        log_memory(self.memory_stats, "project reload after services");

        backtrace::set_oom_context("project reload: load core project");
        let (mut runtime, registry) = {
            let fs_ref = self.fs.borrow();
            ProjectLoader::load_from_root(&*fs_ref, services)
                .map_err(|e| ServerError::Core(format!("Failed to reload core project: {e}")))?
                .into_parts()
        };
        log_memory(self.memory_stats, "project reload after core project");
        backtrace::set_oom_context("project reload: set graphics");
        runtime.set_graphics(Some(self.graphics.clone()));
        // Reload rebuilds the Engine — and with it an empty writer store —
        // so panel state must be restored here too, on the same
        // before-first-frame rule as `new()`. (`apply_project_changes`
        // does NOT rebuild the Engine, which is why an ordinary edit
        // leaves engaged writers alone and touches no file.)
        backtrace::set_oom_context("project reload: restore panel state");
        self.panel_auto_save = {
            let fs_ref = self.fs.borrow();
            panel_state::restore(&*fs_ref, &mut runtime)
        };
        self.panel_state_saved_mutations = runtime.panel_writers().mutations();
        self.panel_state_age_ms = 0;
        self.registry = registry;
        self.runtime = Some(runtime);
        log_memory(self.memory_stats, "project reload after swap");
        backtrace::clear_oom_context();
        Ok(())
    }

    /// How many fs-event batches actually rebuilt project state. A write
    /// into the framework tier (`/.lp/**`) must never move this.
    pub fn applied_refresh_count(&self) -> u64 {
        self.applied_refreshes
    }

    /// Get the last filesystem version processed by this project
    pub fn last_fs_version(&self) -> FsVersion {
        self.last_fs_version
    }

    /// Update the last filesystem version processed by this project
    pub fn update_fs_version(&mut self, version: FsVersion) {
        self.last_fs_version = version;
    }
}

fn log_memory(memory_stats: Option<MemoryStatsFn>, label: &str) {
    if let Some(stats) = memory_stats.and_then(|f| f()) {
        let (free, used) = stats;
        log::info!(
            "[mem] {}: {}k free / {}k used",
            label,
            free / 1024,
            used / 1024
        );
    }
}

fn build_engine_services(
    root_path: TreePath,
    output_provider: Rc<RefCell<dyn OutputProvider>>,
    time_provider: Option<Rc<dyn TimeProvider>>,
    button_service: Option<Rc<dyn ButtonService>>,
    radio_service: Option<Rc<dyn RadioService>>,
) -> EngineServices {
    let mut services = EngineServices::new(root_path);
    services.set_output_provider(Some(Box::new(SharedOutputProvider(output_provider))));
    services.set_time_provider(time_provider);
    services.set_button_service(button_service);
    services.set_radio_service(radio_service);
    services
}

struct SharedOutputProvider(Rc<RefCell<dyn OutputProvider>>);

impl OutputProvider for SharedOutputProvider {
    fn open(
        &self,
        endpoint: &HwEndpointSpec,
        byte_count: u32,
        format: OutputFormat,
        options: Option<OutputDriverOptions>,
    ) -> Result<OutputChannelHandle, lpc_hardware::OutputError> {
        self.0.borrow().open(endpoint, byte_count, format, options)
    }

    fn write(
        &self,
        handle: OutputChannelHandle,
        data: &[u16],
    ) -> Result<(), lpc_hardware::OutputError> {
        self.0.borrow().write(handle, data)
    }

    fn close(&self, handle: OutputChannelHandle) -> Result<(), lpc_hardware::OutputError> {
        self.0.borrow().close(handle)
    }

    // ⚠️ Forward every method, including the defaulted ones. This wrapper
    // once omitted `flush`, so the engine's end-of-frame barrier resolved to
    // the trait's no-op default and never reached the device provider — on
    // the classic ESP32 that silently truncated the last wire of every frame
    // (measured 2026-08-04, G2). A defaulted trait method plus a hand-written
    // delegate is exactly the combination the compiler cannot catch.
    fn flush(&self) -> Result<(), lpc_hardware::OutputError> {
        self.0.borrow().flush()
    }

    fn hardware_generation(&self) -> u64 {
        self.0.borrow().hardware_generation()
    }
}

/// Whether a project-relative fs path is authored content, as opposed to
/// the framework-owned `/.lp/` tier (panel state, meta) that the project
/// itself writes.
///
/// This is the same boundary `lpc_history::is_hashed_path` draws for the
/// canonical package hash and `SnapshotStore` draws for device copies —
/// stated once more here because the artifact refresh path is a third
/// consumer of it, and the one where getting it wrong costs a rebuild
/// loop rather than a wrong hash.
fn is_project_artifact_path(path: &LpPath) -> bool {
    let path = path.as_str();
    path != lpc_history::hash::hash_rules::RESERVED_META_DIR && !path.starts_with("/.lp/")
}

fn project_root_path(name: &str) -> Result<TreePath, ServerError> {
    let mut sanitized = String::new();
    for c in name.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '_' => sanitized.push(c),
            '0'..='9' => sanitized.push(c),
            _ => sanitized.push('_'),
        }
    }

    if sanitized.is_empty() {
        return Err(ServerError::Core(String::from(
            "Project name cannot be empty for core runtime root",
        )));
    }
    if sanitized.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        sanitized.insert(0, '_');
    }

    TreePath::parse(&format!("/{sanitized}.show"))
        .map_err(|e| ServerError::Core(format!("Invalid core runtime root for `{name}`: {e}")))
}

#[cfg(test)]
mod tests {
    use lpc_model::{LpPath, TreePath};

    use super::{is_project_artifact_path, project_root_path};

    #[test]
    fn the_framework_tier_is_not_an_artifact_change() {
        // Panel state is written from inside the tick; if these counted as
        // artifact changes, each save would rebuild the binding graph and
        // the rebuild would fire the next save.
        assert!(!is_project_artifact_path(LpPath::new("/.lp")));
        assert!(!is_project_artifact_path(LpPath::new("/.lp/panel.json")));
        assert!(!is_project_artifact_path(LpPath::new("/.lp/meta.json")));
        assert!(!is_project_artifact_path(LpPath::new("/.lp/nested/x.json")));

        assert!(is_project_artifact_path(LpPath::new("/project.json")));
        assert!(is_project_artifact_path(LpPath::new("/module.json")));
        assert!(is_project_artifact_path(LpPath::new("/shader.glsl")));
        // Not a prefix match on the name: a real artifact may start with
        // the same letters.
        assert!(is_project_artifact_path(LpPath::new("/.lponly.json")));
    }

    #[test]
    fn project_root_path_accepts_demo_folder_names() {
        let path = project_root_path("2026.01.21-03.01.12-test-project").expect("path");

        let expected =
            TreePath::parse("/_2026_01_21_03_01_12_test_project.show").expect("expected path");
        assert_eq!(path, expected);
    }
}
