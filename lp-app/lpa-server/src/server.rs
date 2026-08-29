//! Main server struct for processing messages and managing projects

extern crate alloc;

use crate::error::ServerError;
use crate::handlers;
use crate::project_manager::ProjectManager;
use crate::project_read_source::ServerProjectReadSource;
use alloc::{boxed::Box, format, rc::Rc, string::ToString, sync::Arc, vec::Vec};
use core::cell::RefCell;
use hashbrown::HashMap;
use log;
use lpc_engine::{ButtonService, LpGraphics, RadioService};
use lpc_model::{LpPath, LpPathBuf};
use lpc_shared::output::OutputProvider;
use lpc_shared::time::TimeProvider;
use lpc_shared::transport::{
    ProjectReadStreamSink, ServerTransport, transport_error_is_signalable,
};
use lpc_wire::{ClientRequest, WireMessage, WireServerMessage};
use lpfs::{FsEvent, LpFs};

/// Optional callback returning (free_bytes, used_bytes) for memory logging.
/// Platforms without heap stats (e.g. fw-emu) pass `None`.
pub type MemoryStatsFn = fn() -> Option<(u32, u32)>;

/// Embedder-supplied probe for the *largest allocatable block* (bytes) — the
/// number that matters on a small fragmented arena, where total-free can look
/// healthy while every allocation over a few hundred bytes fails. Embedders
/// that cannot report it (hosts, browser) leave the probe unset and reads are
/// never refused.
pub type ReadHeadroomProbe = fn() -> Option<u32>;

/// Minimum largest-free-block headroom to *begin* serving a ProjectRead.
///
/// Below this, assembly of even a well-behaved streamed read (one slot root or
/// shape entry + one ~16 KiB frame batch + serde transients) risks the
/// infallible-alloc abort path, which RESETS the board
/// (`docs/defects/2026-08-26-project-read-assembly-oom-resets-classic.md`).
/// Refusing with a structured error is always better than resetting.
///
/// Calibrated on silicon at the 2026-08-29 G1 bench walk (classic,
/// zook-dome at 42 ms ticks, ~18–21 KB largest free block loaded): with the
/// gate lowered to 16 KiB the monolithic read passed the gate and still
/// OOM-reset the board — the crash breadcrumb showed a 480 B alloc failing
/// in the *shapes* limb, i.e. the sink's in-memory event batch (sized
/// against the 16 KiB frame budget) plus one atom exhausts such a heap. At
/// 32 KiB the same board refused 5/5 reads in 0.8 s with zero resets. The
/// dominant transient IS roughly one frame batch + atom + slop, so one
/// frame budget × 2 is the honest floor; a board below it (this one could
/// not even JIT its shader — recovery gated it after repeated 768 B compile
/// OOMs) cannot serve any read shape and SHOULD be refused.
pub const PROJECT_READ_MIN_HEADROOM_BYTES: u32 = 32 * 1024;

/// Main server struct for processing client-server messages.
///
/// Message responses are sent through [`ServerTransport`] so large project-read
/// responses can stream without first materializing a full response object.
pub struct LpServer {
    /// Output provider (shared, mutable) for projects
    output_provider: Rc<RefCell<dyn OutputProvider>>,
    /// Project manager for handling multiple projects
    project_manager: ProjectManager,
    /// Base filesystem (server root, projects in `projects/` subdirectory)
    base_fs: Box<dyn LpFs>,
    /// Last frame processing time in microseconds (for theoretical FPS calculation)
    last_frame_time_us: RefCell<Option<u64>>,
    /// Device-level safe-mode output ceiling (0..=255, `None` = no clamp).
    ///
    /// DEVICE state set by the embedder (firmware, from a consumed
    /// boot-control record) — deliberately not project data, and applied to
    /// every engine this server creates, present and future.
    safe_output_clamp: Option<u8>,
    /// The transport's project-read FRAME budget, declared by the embedder
    /// (`set_project_read_frame_budget`). One declaration drives BOTH
    /// derived limits so they can never disagree: the stream sink refuses
    /// events past the frame, and every engine's display-layout budget is
    /// the frame minus the probe-header reserve. `Some(n)` = frames must
    /// fit `n` bytes (serial's 16 KiB default); `None` = the link has no
    /// meaningful frame limit (in-proc, websocket) — layouts are always
    /// answered and the sink gets a generous runaway ceiling instead.
    project_read_frame_budget: Option<usize>,
    /// Optional memory stats callback for logging (ESP32 passes impl, others pass None)
    memory_stats: Option<MemoryStatsFn>,
    /// Optional largest-free-block probe backing the ProjectRead headroom
    /// refusal gate. Unset (hosts/browser) = reads are never refused.
    read_headroom_probe: Option<ReadHeadroomProbe>,
    /// Optional time provider for perf timing (e.g. shader comp). ESP32/emu pass, others None.
    time_provider: Option<Rc<dyn TimeProvider>>,
    /// Optional hardware button service for input nodes.
    button_service: Option<Rc<dyn ButtonService>>,
    /// Optional hardware radio service for radio nodes.
    radio_service: Option<Rc<dyn RadioService>>,
    /// Shader backend (Cranelift, WASM, …).
    graphics: Arc<dyn LpGraphics>,
    /// Identity/version/capability payload answered to
    /// `ClientRequest::Hello` and sent unsolicited by embedder loops.
    ///
    /// Two halves with two owners. The CAPABILITY half — `build.features`
    /// and `hardware` — is computed in the constructor from the engine's
    /// own `cfg!` truth and the services injected here, so every embedder
    /// reports it correctly including the two that never state an identity
    /// (`fw-emu`, `lp-cli`). The IDENTITY half — provenance and the
    /// stamped uid — is injected by the embedder via
    /// [`LpServer::set_hello_identity`] (sans-IO: the server never reads
    /// git/fs/env state itself) and defaults to `"unknown"`.
    hello: lpc_wire::ServerHello,
    /// Consecutive per-project tick failures, so a PERSISTENT error states
    /// itself instead of restating itself every frame.
    ///
    /// A tick error is usually not a one-frame event: an unsupported
    /// builtin on the current tier, a node the graph cannot render, a
    /// missing sampler — these fail identically on every frame until the
    /// project or the tier changes. Logged per frame that is ~60 warnings
    /// a second, each with a full stack trace in the browser, which buries
    /// the very first occurrence that explains the cause.
    tick_failures: HashMap<lpc_wire::WireProjectHandle, u32>,
}

/// After the first failure, restate a persistent tick error only every
/// this many consecutive frames (~8s at 60fps) — enough to show it is
/// still happening without drowning the console.
const TICK_ERROR_RESTATE_EVERY: u32 = 512;

/// The wire proto this build REPORTS.
///
/// Normally [`lpc_wire::WIRE_PROTO_VERSION`]. A `fixture-old-proto` build
/// reports one LESS, so a current Studio classifies it Incompatible
/// (proto-mismatch) — the s4 device scenario, reproducible from source
/// instead of from an archived binary. Never enable this in a released
/// image.
const fn fixture_proto() -> u32 {
    if cfg!(feature = "fixture-old-proto") {
        lpc_wire::WIRE_PROTO_VERSION - 1
    } else {
        lpc_wire::WIRE_PROTO_VERSION
    }
}

impl LpServer {
    /// Create a new LpServer instance
    ///
    /// # Arguments
    ///
    /// * `output_provider` - Shared output provider for projects (Rc<RefCell> for no_std compatibility)
    /// * `base_fs` - Base filesystem (server root, projects stored in `projects_base_dir` subdirectory)
    /// * `projects_base_dir` - Base directory for projects (e.g., "projects/")
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// extern crate alloc;
    /// use lpc_model::AsLpPath;
    /// use lpa_server::LpServer;
    /// use lpfs::LpFsStd;
    /// use lpc_shared::output::MemoryOutputProvider;
    /// use alloc::{boxed::Box, rc::Rc, sync::Arc};
    /// use core::cell::RefCell;
    ///
    /// let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));
    /// let base_fs = Box::new(LpFsStd::new("/path/to/server/root".into()));
    /// let graphics = Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
    ///     lpa_server::DEVICE_SHADER_FRONTEND,
    /// ));
    /// let server = LpServer::new(
    ///     output_provider,
    ///     base_fs,
    ///     "projects/".as_path(),
    ///     None,
    ///     None,
    ///     graphics,
    /// );
    /// ```
    pub fn new(
        output_provider: Rc<RefCell<dyn OutputProvider>>,
        base_fs: Box<dyn LpFs>,
        projects_base_dir: &LpPath,
        memory_stats: Option<MemoryStatsFn>,
        time_provider: Option<Rc<dyn TimeProvider>>,
        graphics: Arc<dyn LpGraphics>,
    ) -> Self {
        Self::new_with_button_service(
            output_provider,
            base_fs,
            projects_base_dir,
            memory_stats,
            time_provider,
            None,
            graphics,
        )
    }

    pub fn new_with_button_service(
        output_provider: Rc<RefCell<dyn OutputProvider>>,
        base_fs: Box<dyn LpFs>,
        projects_base_dir: &LpPath,
        memory_stats: Option<MemoryStatsFn>,
        time_provider: Option<Rc<dyn TimeProvider>>,
        button_service: Option<Rc<dyn ButtonService>>,
        graphics: Arc<dyn LpGraphics>,
    ) -> Self {
        Self::new_with_hardware_services(
            output_provider,
            base_fs,
            projects_base_dir,
            memory_stats,
            time_provider,
            button_service,
            None,
            graphics,
        )
    }

    pub fn new_with_hardware_services(
        output_provider: Rc<RefCell<dyn OutputProvider>>,
        base_fs: Box<dyn LpFs>,
        projects_base_dir: &LpPath,
        memory_stats: Option<MemoryStatsFn>,
        time_provider: Option<Rc<dyn TimeProvider>>,
        button_service: Option<Rc<dyn ButtonService>>,
        radio_service: Option<Rc<dyn RadioService>>,
        graphics: Arc<dyn LpGraphics>,
    ) -> Self {
        let project_manager = ProjectManager::new(projects_base_dir);
        let hardware = lpc_wire::HardwareFacts {
            radio: radio_service.is_some(),
            button: button_service.is_some(),
            // Nothing on the device writes a board identity yet; the field
            // becomes populatable when provisioning writes `/hardware.json`
            // (board-selection roadmap M5).
            board_id: None,
            // The measured LED envelope lands through
            // `set_total_led_budget` — only the embedder holds the board
            // manifest (same posture as the efuse facts below).
            total_led_budget: None,
            // Efuse facts: only the embedder can read them, so they land
            // through `set_hardware_identity` (like build provenance).
            base_mac: None,
            chip_revision: None,
            eui64: None,
        };
        let features = server_features(&hardware, graphics.backend_name());
        Self {
            output_provider,
            project_manager,
            base_fs,
            last_frame_time_us: RefCell::new(None),
            safe_output_clamp: None,
            project_read_frame_budget: Some(lpc_wire::PROJECT_READ_FRAME_MAX_BYTES),
            memory_stats,
            read_headroom_probe: None,
            time_provider,
            button_service,
            radio_service,
            graphics,
            hello: lpc_wire::ServerHello {
                proto: fixture_proto(),
                build: lpc_wire::BuildFacts {
                    features,
                    package: "unknown".to_string(),
                    commit: "unknown".to_string(),
                    dirty: false,
                    profile: "unknown".to_string(),
                },
                hardware,
                device_uid: None,
            },
            tick_failures: HashMap::new(),
        }
    }

    /// Inject the embedder-owned half of the hello: build provenance, the
    /// stamped device uid, and the wire proto to report. Call once at
    /// construction time, before the server loop starts serving.
    ///
    /// Deliberately CANNOT reach the capability half (`build.features`,
    /// `hardware`): those are derived in the constructor from the engine's
    /// `cfg!` truth and the injected services, so a server whose embedder
    /// never calls this still reports its abilities honestly.
    pub fn set_hello_identity(&mut self, identity: lpc_wire::HelloIdentity) {
        let lpc_wire::HelloIdentity {
            proto,
            package,
            commit,
            dirty,
            profile,
            device_uid,
        } = identity;
        // A fixture build keeps LYING even when the embedder states the
        // truth — the whole point is that the wire reports the wrong
        // version (see the `fixture-old-proto` feature).
        self.hello.proto = if cfg!(feature = "fixture-old-proto") {
            fixture_proto()
        } else {
            proto
        };
        self.hello.build.package = package;
        self.hello.build.commit = commit;
        self.hello.build.dirty = dirty;
        self.hello.build.profile = profile;
        self.hello.device_uid = device_uid;
    }

    /// Inject the chip-level identity the server cannot derive: the
    /// factory MAC, the silicon revision, and the 802.15.4 EUI-64 where
    /// the chip has one.
    ///
    /// These live in efuse, so only the embedder can read them — the same
    /// reason build provenance arrives through
    /// [`Self::set_hello_identity`] rather than being derived. Embedders
    /// with no efuse (the host server, the browser worker, `lp-cli`)
    /// never call this and honestly report `None`.
    ///
    /// Call at construction, beside [`Self::set_hello_identity`].
    /// Stamp the board manifest's measured total-LED envelope into the
    /// hello (the embedder is the only party holding the manifest — same
    /// posture as [`Self::set_hardware_identity`]).
    pub fn set_total_led_budget(&mut self, budget: Option<u32>) {
        self.hello.hardware.total_led_budget = budget;
    }

    pub fn set_hardware_identity(&mut self, identity: lpc_wire::HardwareIdentity) {
        let lpc_wire::HardwareIdentity {
            base_mac,
            chip_revision,
            eui64,
        } = identity;
        self.hello.hardware.base_mac = base_mac;
        self.hello.hardware.chip_revision = chip_revision;
        self.hello.hardware.eui64 = eui64;
    }

    /// Declare embedder-owned features the server cannot derive from
    /// anything it holds.
    ///
    /// Today that is exactly [`LpFeature::ShaderF32`]: whether the shader
    /// engine linked into the image does native f32 math is a property of
    /// the embedder's Cargo graph (`float-f32`), invisible from the
    /// `Arc<dyn LpGraphics>` and from `lpc-engine`'s gates. Features the
    /// server DOES know (the engine's node runtimes, the graphics backend,
    /// the wired services) are computed in the constructor and are not
    /// affected by this call; declaring one again is harmless.
    ///
    /// Call at construction, beside [`Self::set_hello_identity`]. This
    /// mirrors the firmware manifest macro, where the embedder likewise
    /// names only its own facts.
    pub fn declare_embedder_features(&mut self, features: &[lpc_model::LpFeature]) {
        let declared = &mut self.hello.build.features;
        for feature in features {
            if !declared.contains(feature) {
                declared.push(*feature);
            }
        }
        declared.sort_unstable_by_key(|feature| {
            lpc_model::LpFeature::ALL
                .iter()
                .position(|candidate| candidate == feature)
                .unwrap_or(usize::MAX)
        });
    }

    /// The hello payload answered to `ClientRequest::Hello` and emitted
    /// unsolicited (id 0) by embedder loops.
    pub fn hello(&self) -> &lpc_wire::ServerHello {
        &self.hello
    }

    /// Advance loaded projects by one frame without processing client messages.
    pub fn advance_frame(&mut self, delta_ms: u32) -> Result<(), ServerError> {
        // Process filesystem changes for all loaded projects
        // Collect project info first to avoid borrowing issues
        let project_info: Vec<_> = self
            .project_manager
            .list_loaded_projects()
            .iter()
            .map(|p| (p.handle, p.path.clone()))
            .collect();

        log::debug!(
            "LpServer::tick: Found {} loaded projects",
            project_info.len()
        );

        // Collect changes per project
        let mut project_changes_map: HashMap<_, Vec<FsEvent>> = HashMap::new();

        for (handle, project_path) in &project_info {
            if let Some(project) = self.project_manager.get_project(*handle) {
                let last_version = project.last_fs_version();

                // Query changes from base_fs
                let base_changes = self.base_fs().get_changes_since(last_version);

                // If no changes, skip this project
                if base_changes.is_empty() {
                    continue;
                }

                // Filter changes for this project
                // Build project prefix path using join - ensure it ends with /
                let project_prefix_buf = LpPathBuf::from("/").join(project_path.as_str()).join("");
                let project_prefix = project_prefix_buf.as_str();
                let project_changes: Vec<FsEvent> = base_changes
                    .into_iter()
                    .filter_map(|change| {
                        // Use LpPath to strip prefix and normalize
                        if let Some(stripped) = change.path.strip_prefix(project_prefix) {
                            Some(FsEvent {
                                path: stripped.to_path_buf(),
                                kind: change.kind,
                            })
                        } else {
                            None
                        }
                    })
                    .collect();

                if !project_changes.is_empty() {
                    project_changes_map.insert(*handle, project_changes);
                }
            }
        }

        // Capture the next version before refresh applies anything. The events
        // in this batch are all older than this marker.
        let current_version = self.base_fs().current_version();

        // Now apply changes to projects (mutable borrows)
        for (handle, project_changes) in project_changes_map {
            if let Some(project) = self.project_manager.get_project_mut(handle) {
                if let Err(_e) = project.refresh_artifacts(&project_changes) {
                    // Log error but continue with other projects
                    // Note: In no_std context, errors are silently ignored
                    // Errors will be visible when clients read project state.
                } else {
                    // Advance past the batch marker so the same events are not
                    // returned again by get_changes_since, which is inclusive.
                    project.update_fs_version(current_version.next());
                }
            }
        }

        // Tick all loaded projects
        // Tick each project's runtime BEFORE processing incoming messages.
        // This ensures project read requests see the current frame's data.
        log::debug!("LpServer::tick: Ticking {} projects", project_info.len());
        // Disjoint field borrows: the tick loop holds `project_manager`
        // mutably while the failure ledger is read and written beside it.
        let tick_failures = &mut self.tick_failures;
        for (handle, path) in &project_info {
            if let Some(project) = self.project_manager.get_project_mut(*handle) {
                log::debug!(
                    "LpServer::tick: Ticking project {} (path: {}, delta_ms: {})",
                    project.name(),
                    path,
                    delta_ms
                );
                // One project's failure never stops the others; clients
                // see it when they sync or query project state.
                //
                // A tick error is normally PERSISTENT (it re-fails every
                // frame until the project or the tier changes), so the
                // log states it and then holds its tongue — see
                // `tick_failures`.
                match project.tick(delta_ms) {
                    Ok(()) => {
                        log::trace!("LpServer::tick: Project {} tick succeeded", project.name());
                        if let Some(failures) = tick_failures.remove(handle) {
                            log::info!(
                                "LpServer::tick: Project {} recovered after {} failed frame(s)",
                                project.name(),
                                failures
                            );
                        }
                    }
                    Err(e) => {
                        let failures = tick_failures.entry(*handle).or_insert(0);
                        *failures += 1;
                        if *failures == 1 {
                            log::warn!(
                                "LpServer::tick: Project {} tick error: {:?}",
                                project.name(),
                                e
                            );
                        } else if *failures % TICK_ERROR_RESTATE_EVERY == 0 {
                            log::warn!(
                                "LpServer::tick: Project {} tick error persists \
                                 ({} consecutive frames): {:?}",
                                project.name(),
                                failures,
                                e
                            );
                        }
                    }
                }
            } else {
                log::warn!(
                    "LpServer::tick: Project handle {} not found",
                    handle.as_i32()
                );
            }
        }
        // Handles are minted monotonically and never reused, so a project
        // that was unloaded mid-failure would otherwise keep its ledger
        // entry for the process's lifetime.
        if !tick_failures.is_empty() {
            tick_failures
                .retain(|handle, _| project_info.iter().any(|(loaded, _)| loaded == handle));
        }

        // Log frame IDs after ticking (for debugging frame synchronization)
        for (handle, _) in &project_info {
            if let Some(project) = self.project_manager.get_project(*handle) {
                log::debug!(
                    "LpServer::tick: Project {} revision: {}",
                    project.name(),
                    project.engine().revision().as_i64()
                );
            }
        }

        Ok(())
    }

    /// Tick projects and send incoming-message responses through a transport.
    ///
    /// This avoids materializing large project-read responses before transport
    /// serialization. Simple transports may still fall back to an in-memory
    /// implementation internally, but firmware transports can stream directly.
    pub async fn tick_and_send<T: ServerTransport>(
        &mut self,
        delta_ms: u32,
        incoming: Vec<WireMessage>,
        transport: &mut T,
    ) -> Result<usize, ServerError> {
        self.advance_frame(delta_ms)?;

        let mut response_count = 0usize;
        for message in incoming {
            match message {
                WireMessage::Client(client_msg) => {
                    let msg_id = client_msg.id;
                    match client_msg.msg {
                        ClientRequest::ProjectRead { handle, request } => {
                            let sink_frame_budget = self.sink_frame_budget();
                            // Refusal-not-reset: if the heap cannot afford
                            // even a well-behaved streamed read, fail the
                            // request with a structured terminal error instead
                            // of letting infallible alloc abort-reset the
                            // board mid-assembly.
                            if let Some(headroom) =
                                self.read_headroom_probe.and_then(|probe| probe())
                                && headroom < PROJECT_READ_MIN_HEADROOM_BYTES
                            {
                                let mut sink = ProjectReadStreamSink::with_max_bytes(
                                    transport,
                                    msg_id,
                                    sink_frame_budget,
                                );
                                let message = format!(
                                    "read refused: heap headroom too low (largest free block \
                                     {headroom} B < {PROJECT_READ_MIN_HEADROOM_BYTES} B); narrow \
                                     the query (include_slots:false, one probe per read, or page \
                                     nodes by id) and retry",
                                );
                                log::warn!("tick_and_send: {message}");
                                if let Err(send_error) = sink.send_terminal_error(message).await {
                                    log::warn!(
                                        "tick_and_send: failed to send read-refusal error for \
                                         id={msg_id}: {send_error}"
                                    );
                                }
                                response_count += 1;
                                continue;
                            }
                            let mut server_status = self.runtime_status();
                            let Some(project) = self.project_manager.get_project_mut(handle) else {
                                transport
                                    .send(WireServerMessage::new(
                                        msg_id,
                                        lpc_wire::server::ServerMsgBody::Error {
                                            error: format!(
                                                "{}",
                                                ServerError::ProjectNotFound(format!(
                                                    "handle {}",
                                                    handle.id()
                                                ))
                                            ),
                                        },
                                    ))
                                    .await
                                    .map_err(|error| ServerError::Core(format!("{error}")))?;
                                response_count += 1;
                                continue;
                            };
                            // The P11 toggle's read path: panel auto-save
                            // is per-project (`.lp/state.json`) and lives
                            // on the wrapper, not the engine, so it is
                            // stamped here — once the read's project is
                            // known — rather than in `runtime_status`.
                            server_status.panel_auto_save = Some(project.panel_auto_save());
                            let mut source =
                                ServerProjectReadSource::new(project, Some(server_status));
                            let mut sink = ProjectReadStreamSink::with_max_bytes(
                                transport,
                                msg_id,
                                sink_frame_budget,
                            );
                            let stream_result =
                                source.stream_project_read_events(request, &mut sink).await;
                            match stream_result {
                                Ok(()) => {
                                    sink.finish()
                                        .await
                                        .map_err(|error| ServerError::Core(format!("{error}")))?;
                                }
                                Err(error) => {
                                    // Signalable failures (event too large for an
                                    // empty frame, other serialization/budget
                                    // errors) still have a live connection: send a
                                    // terminal `Error` frame for this request id
                                    // and continue the tick. Transport-write
                                    // failures cannot be signaled and propagate.
                                    match classify_project_read_stream_error(error) {
                                        ProjectReadStreamOutcome::Signalable(message) => {
                                            if let Err(send_error) =
                                                sink.send_terminal_error(message).await
                                            {
                                                log::warn!(
                                                    "tick_and_send: failed to send terminal \
                                                     project-read error for id={msg_id}: \
                                                     {send_error}"
                                                );
                                            }
                                        }
                                        ProjectReadStreamOutcome::Fatal(server_error) => {
                                            return Err(server_error);
                                        }
                                    }
                                }
                            }
                            response_count += 1;
                        }
                        msg => {
                            // Every request id gets exactly one response even
                            // when the handler fails — a propagated error here
                            // would drop the frame and leave the client
                            // awaiting forever. Only transport-send failures
                            // abort the tick.
                            let response = match handlers::handle_client_message(
                                &mut self.project_manager,
                                &mut *self.base_fs,
                                &self.output_provider,
                                self.memory_stats.as_ref(),
                                self.time_provider.clone(),
                                self.button_service.clone(),
                                self.radio_service.clone(),
                                self.graphics.clone(),
                                &self.hello,
                                lpc_wire::ClientMessage { id: msg_id, msg },
                            ) {
                                Ok(response) => response,
                                Err(error) => {
                                    log::warn!(
                                        "tick_and_send: request id={msg_id} failed: {error}"
                                    );
                                    WireServerMessage::new(
                                        msg_id,
                                        lpc_wire::server::ServerMsgBody::Error {
                                            error: format!("{error}"),
                                        },
                                    )
                                }
                            };
                            transport
                                .send(response)
                                .await
                                .map_err(|error| ServerError::Core(format!("{error}")))?;
                            response_count += 1;
                        }
                    }
                }
                WireMessage::Server(_) => {
                    return Err(ServerError::Core(
                        "Received server message on server side".to_string(),
                    ));
                }
            }
        }

        Ok(response_count)
    }

    /// Get a reference to the base filesystem
    pub fn base_fs(&self) -> &dyn LpFs {
        &*self.base_fs
    }

    /// Get a reference to the project manager
    pub fn project_manager(&self) -> &ProjectManager {
        &self.project_manager
    }

    /// Get a mutable reference to the project manager
    pub fn project_manager_mut(&mut self) -> &mut ProjectManager {
        &mut self.project_manager
    }

    /// Get a mutable reference to the base filesystem
    ///
    /// This is primarily for testing purposes where we need mutable access
    /// to load projects.
    pub fn base_fs_mut(&mut self) -> &mut dyn LpFs {
        &mut *self.base_fs
    }

    /// Get the output provider (for loading projects)
    pub fn output_provider(&self) -> &Rc<RefCell<dyn OutputProvider>> {
        &self.output_provider
    }

    /// Get the memory stats callback
    /// Install the largest-free-block probe the ProjectRead headroom gate
    /// consults (see [`PROJECT_READ_MIN_HEADROOM_BYTES`]). Unset = never
    /// refuse.
    pub fn set_read_headroom_probe(&mut self, probe: Option<ReadHeadroomProbe>) {
        self.read_headroom_probe = probe;
    }

    pub fn memory_stats(&self) -> Option<MemoryStatsFn> {
        self.memory_stats
    }

    /// Load a project (internal use, e.g. boot auto-load).
    ///
    /// Avoids multiple borrows when caller needs to pass base_fs, output_provider, etc.
    pub fn load_project(
        &mut self,
        path: &lpfs::lp_path::LpPath,
    ) -> Result<lpc_wire::WireProjectHandle, ServerError> {
        let handle = self.project_manager.load_project(
            path,
            &mut *self.base_fs,
            self.output_provider.clone(),
            self.memory_stats,
            self.time_provider.clone(),
            self.button_service.clone(),
            self.radio_service.clone(),
            self.graphics.clone(),
        )?;
        // The clamp and the frame budget are device state: every engine
        // wears them, including this freshly created one.
        let engine_budget = self.engine_display_layout_budget();
        if let Some(project) = self.project_manager.get_project_mut(handle) {
            project
                .engine_mut()
                .set_safe_output_clamp(self.safe_output_clamp);
            project
                .engine_mut()
                .set_display_layout_budget(engine_budget);
        }
        Ok(handle)
    }

    /// Set (or clear) the device-level safe-mode output ceiling and apply it
    /// to every loaded project's engine. Future loads inherit it too.
    pub fn set_safe_output_clamp(&mut self, level: Option<u8>) {
        self.safe_output_clamp = level;
        let handles: alloc::vec::Vec<_> = self
            .project_manager
            .list_loaded_projects()
            .into_iter()
            .map(|loaded| loaded.handle)
            .collect();
        for handle in handles {
            if let Some(project) = self.project_manager.get_project_mut(handle) {
                project.engine_mut().set_safe_output_clamp(level);
            }
        }
    }

    /// The active device-level safe-mode output ceiling, for heartbeat
    /// reporting (clients surface the safe-mode state and its exit).
    pub fn safe_output_clamp(&self) -> Option<u8> {
        self.safe_output_clamp
    }

    /// Declare the transport's project-read frame budget and apply the
    /// derived display-layout budget to every loaded engine. Future loads
    /// inherit it too.
    ///
    /// The default is the embedded serial frame
    /// ([`lpc_wire::PROJECT_READ_FRAME_MAX_BYTES`]) — the smallest link —
    /// so only embedders with bigger pipes (in-proc hosts, websocket, the
    /// browser sim) need to opt out with `None`. Fail-safe direction: an
    /// un-plumbed host refuses big layouts rather than wedging a serial
    /// link.
    pub fn set_project_read_frame_budget(&mut self, budget: Option<usize>) {
        self.project_read_frame_budget = budget;
        let engine_budget = self.engine_display_layout_budget();
        let handles: alloc::vec::Vec<_> = self
            .project_manager
            .list_loaded_projects()
            .into_iter()
            .map(|loaded| loaded.handle)
            .collect();
        for handle in handles {
            if let Some(project) = self.project_manager.get_project_mut(handle) {
                project
                    .engine_mut()
                    .set_display_layout_budget(engine_budget);
            }
        }
    }

    /// The display-layout byte budget engines derive from the declared
    /// frame budget: frame minus the probe-header reserve, `None` when the
    /// link is unbounded.
    fn engine_display_layout_budget(&self) -> Option<usize> {
        self.project_read_frame_budget
            .map(|frame| frame.saturating_sub(lpc_wire::PROJECT_READ_PROBE_HEADER_RESERVE_BYTES))
    }

    /// The stream sink's per-event ceiling for this link. A bounded link
    /// enforces its declared frame; an unbounded one still gets a generous
    /// runaway ceiling so a berserk serializer cannot OOM the host.
    fn sink_frame_budget(&self) -> usize {
        const UNBOUNDED_LINK_RUNAWAY_CEILING: usize = 4 * 1024 * 1024;
        self.project_read_frame_budget
            .unwrap_or(UNBOUNDED_LINK_RUNAWAY_CEILING)
    }

    /// Set the last frame processing time (called by server loop)
    ///
    /// # Arguments
    ///
    /// * `time_us` - Frame processing time in microseconds
    pub fn set_last_frame_time(&self, time_us: u64) {
        *self.last_frame_time_us.borrow_mut() = Some(time_us);
    }

    /// Get the last frame processing time in microseconds
    ///
    /// Returns `None` if no frame has been processed yet.
    pub fn last_frame_time_us(&self) -> Option<u64> {
        *self.last_frame_time_us.borrow()
    }

    /// Get theoretical FPS based on last frame processing time
    ///
    /// Returns `None` if no frame has been processed yet.
    /// Returns theoretical FPS as `1000000.0 / frame_time_us`.
    pub fn theoretical_fps(&self) -> Option<f32> {
        self.last_frame_time_us()
            .map(|time_us| 1_000_000.0 / time_us as f32)
    }

    fn runtime_status(&self) -> lpc_wire::ServerRuntimeStatus {
        let memory = self.memory_stats.and_then(|memory_stats| {
            memory_stats().map(|(free_bytes, used_bytes)| lpc_wire::MemoryStats {
                free_bytes,
                used_bytes,
                total_bytes: free_bytes.saturating_add(used_bytes),
                // Fragmentation evidence, when the embedder installed the
                // headroom probe (the same number the refusal gate consults).
                largest_free_block: self.read_headroom_probe.and_then(|probe| probe()),
                oom_retry_saves: None,
            })
        });
        lpc_wire::ServerRuntimeStatus {
            theoretical_fps: self.theoretical_fps(),
            last_frame_time_us: self.last_frame_time_us(),
            memory,
            // Per-project, so it is stamped by the read dispatch once the
            // handle resolves — the server loop has no one project.
            panel_auto_save: None,
        }
    }
}

/// Result of classifying a project-read event-stream failure.
enum ProjectReadStreamOutcome {
    /// The connection is still alive; send a terminal `Error` frame carrying
    /// this message for the request id, then continue the tick.
    Signalable(alloc::string::String),
    /// A transport-write failure that cannot be signaled; abort the tick.
    Fatal(ServerError),
}

/// Classify a project-read stream failure into signalable vs. fatal.
///
/// Signalable failures are engine-side protocol/budget errors and sink
/// serialization/budget failures — the write path is still usable, so the
/// server best-effort emits a terminal [`lpc_wire::ProjectReadEvent::Error`].
/// Sink transport-write failures (connection lost, other) are fatal and
/// propagate as before.
fn classify_project_read_stream_error(
    error: lpc_engine::ProjectReadEventStreamError<lpc_wire::TransportError>,
) -> ProjectReadStreamOutcome {
    match error {
        lpc_engine::ProjectReadEventStreamError::Protocol(message) => {
            ProjectReadStreamOutcome::Signalable(message)
        }
        lpc_engine::ProjectReadEventStreamError::Sink(transport_error) => {
            if transport_error_is_signalable(&transport_error) {
                ProjectReadStreamOutcome::Signalable(format!("{transport_error}"))
            } else {
                ProjectReadStreamOutcome::Fatal(ServerError::Core(format!("{transport_error}")))
            }
        }
    }
}

/// The features this server can honestly report about itself: the engine's
/// own `cfg!`-derived list, plus the two facts the server holds directly —
/// which hardware services were injected and which graphics backend it was
/// handed. Embedder-only facts (`shader.f32`) arrive via
/// [`LpServer::declare_embedder_features`].
///
/// Result is in [`lpc_model::LpFeature::ALL`] order, matching the embedded
/// firmware manifest core's ordering, so the two projections of the same
/// truth read identically.
fn server_features(
    hardware: &lpc_wire::HardwareFacts,
    backend_name: &str,
) -> Vec<lpc_model::LpFeature> {
    use lpc_model::LpFeature;

    let mut features = lpc_engine::features::supported_features();
    if hardware.button {
        features.push(LpFeature::SvcButton);
    }
    if hardware.radio {
        features.push(LpFeature::SvcRadioEspnow);
    }
    if let Some(gfx) = graphics_feature(backend_name) {
        features.push(gfx);
    }
    features.sort_unstable_by_key(|feature| {
        LpFeature::ALL
            .iter()
            .position(|candidate| candidate == feature)
            .unwrap_or(usize::MAX)
    });
    features
}

/// The `gfx.*` feature a backend label names, or `None` for a backend
/// outside the registry (test doubles, the timing harness's passthrough).
///
/// The match is over [`lpc_model::LpFeature`], not over the label, so a new
/// feature variant is a compile error here until someone decides whether it
/// is a graphics backend and, if so, which labels it answers for. Labels
/// are matched by prefix because the LPVM family names its engine
/// (`lpvm-native::rt_jit`, `lpvm-wasm::rt_wasmtime`, …).
fn graphics_feature(backend_name: &str) -> Option<lpc_model::LpFeature> {
    use lpc_model::LpFeature;

    /// The backend-label prefixes a feature answers for; `None` for every
    /// non-graphics feature.
    const fn label_prefix(feature: LpFeature) -> Option<&'static str> {
        match feature {
            LpFeature::GfxLpvm => Some("lpvm-"),
            LpFeature::GfxNull => Some("null-graphics"),
            LpFeature::GfxWgpu => Some("wgpu"),
            LpFeature::NodeButton
            | LpFeature::NodeClock
            | LpFeature::NodeFluid
            | LpFeature::NodeFixture
            | LpFeature::NodePlaylist
            | LpFeature::NodeRadio
            | LpFeature::NodeShader
            | LpFeature::NodeTexture
            | LpFeature::SvcButton
            | LpFeature::SvcRadioEspnow
            | LpFeature::DiagUnwind
            | LpFeature::ShaderF32 => None,
        }
    }

    LpFeature::ALL.iter().copied().find(|feature| {
        label_prefix(*feature).is_some_and(|prefix| backend_name.starts_with(prefix))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::LpFeature;

    /// The graphics backend is read off the injected backend's own label,
    /// and an unregistered label reports nothing rather than guessing.
    #[test]
    fn graphics_backends_map_to_their_features() {
        assert_eq!(
            graphics_feature("lpvm-native::rt_jit"),
            Some(LpFeature::GfxLpvm)
        );
        assert_eq!(
            graphics_feature("lpvm-wasm::rt_wasmtime"),
            Some(LpFeature::GfxLpvm)
        );
        assert_eq!(graphics_feature("wgpu"), Some(LpFeature::GfxWgpu));
        assert_eq!(graphics_feature("null-graphics"), Some(LpFeature::GfxNull));
        assert_eq!(graphics_feature("test-double"), None);
    }

    /// Wired services become `svc.*` features beside the engine's own list,
    /// and an unwired service simply does not appear.
    #[test]
    fn wired_services_become_features() {
        let both = lpc_wire::HardwareFacts {
            radio: true,
            button: true,
            board_id: None,
            ..Default::default()
        };
        let features = server_features(&both, "lpvm-native::rt_jit");
        assert!(features.contains(&LpFeature::SvcButton));
        assert!(features.contains(&LpFeature::SvcRadioEspnow));
        assert!(features.contains(&LpFeature::GfxLpvm));

        let neither = lpc_wire::HardwareFacts {
            radio: false,
            button: false,
            board_id: None,
            ..Default::default()
        };
        let features = server_features(&neither, "lpvm-native::rt_jit");
        assert!(!features.contains(&LpFeature::SvcButton));
        assert!(!features.contains(&LpFeature::SvcRadioEspnow));
    }

    /// The reported list keeps `LpFeature::ALL` order, so hello and the
    /// embedded manifest core read identically.
    #[test]
    fn reported_features_are_in_registry_order() {
        let hardware = lpc_wire::HardwareFacts {
            radio: true,
            button: true,
            board_id: None,
            ..Default::default()
        };
        let features = server_features(&hardware, "lpvm-native::rt_jit");
        let mut ordered = features.clone();
        ordered.sort_unstable_by_key(|feature| {
            LpFeature::ALL.iter().position(|c| c == feature).unwrap()
        });
        assert_eq!(features, ordered);
    }
}
