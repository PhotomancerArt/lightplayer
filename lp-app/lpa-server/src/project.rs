//! Project wrapper for managing a single project instance

extern crate alloc;

use crate::error::ServerError;
use crate::server::MemoryStatsFn;
use alloc::{boxed::Box, format, rc::Rc, string::String, sync::Arc};
use core::cell::RefCell;
use lpc_engine::{ButtonService, Engine, EngineServices, LpGraphics, ProjectLoader, RadioService};
use lpc_hardware::HwEndpointSpec;
use lpc_model::{LpPath, LpPathBuf, TreePath, current_revision};
use lpc_registry::{ParseCtx, ProjectRegistry};
use lpc_shared::backtrace;
use lpc_shared::output::{OutputChannelHandle, OutputDriverOptions, OutputFormat, OutputProvider};
use lpc_shared::time::TimeProvider;
use lpc_wire::{
    ProjectReadRequest, ProjectReadResponse, WireOverlayCommitRequest, WireOverlayCommitResponse,
    WireOverlayMutationRequest, WireOverlayMutationResponse, WireOverlayReadResponse,
    WireProjectInventoryReadResponse,
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

        backtrace::set_oom_context("project new: build wrapper");
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

    pub(crate) fn runtime_read_parts(&mut self) -> (&mut Engine, &ProjectRegistry) {
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
        runtime
            .tick(registry, delta_ms)
            .map_err(|e| ServerError::Core(format!("{e}")))
    }

    pub fn read_project(&mut self, request: ProjectReadRequest) -> ProjectReadResponse {
        let registry = &self.registry;
        let runtime = self
            .runtime
            .as_mut()
            .expect("project runtime is only absent while reloading");
        runtime.read_project(registry, request)
    }

    pub fn read_overlay(&self) -> WireOverlayReadResponse {
        WireOverlayReadResponse::new(self.registry.overlay().get().clone())
    }

    pub fn read_inventory(&self) -> WireProjectInventoryReadResponse {
        let index = self.engine().project_runtime_index();
        WireProjectInventoryReadResponse::from_inventory_with_runtime_ids(
            self.registry.inventory(),
            |use_location| index.node_id(use_location),
        )
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
        Ok(WireOverlayMutationResponse::new(result.commands))
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
        Ok(WireOverlayCommitResponse::new(result))
    }

    pub fn refresh_artifacts(&mut self, events: &[FsEvent]) -> Result<(), ServerError> {
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
        self.registry = registry;
        self.runtime = Some(runtime);
        log_memory(self.memory_stats, "project reload after swap");
        backtrace::clear_oom_context();
        Ok(())
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
    use lpc_model::TreePath;

    use super::project_root_path;

    #[test]
    fn project_root_path_accepts_demo_folder_names() {
        let path = project_root_path("2026.01.21-03.01.12-test-project").expect("path");

        let expected =
            TreePath::parse("/_2026_01_21_03_01_12_test_project.show").expect("expected path");
        assert_eq!(path, expected);
    }
}
