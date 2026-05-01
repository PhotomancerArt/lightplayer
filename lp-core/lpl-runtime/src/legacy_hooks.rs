//! Legacy `ProjectRuntime` integration (init, tick, fs sync, client protocol details).
#![allow(
    clippy::missing_errors_doc,
    reason = "legacy internal module, docs deferred"
)]
#![allow(
    clippy::too_many_lines,
    reason = "legacy runtime functions, refactor deferred"
)]

use crate::nodes::{FixtureRuntime, OutputRuntime, ShaderRuntime, TextureRuntime};
use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::{vec, vec::Vec};
use core::cell::RefCell;
use log;
use lp_perf::EVENT_FRAME;
use lpc_engine::error::Error;
use lpc_engine::nodes::LegacyNodeRuntime;
use lpc_engine::output::OutputProvider;
use lpc_engine::panic_node::catch_node_panic;
use lpc_engine::legacy_project::LegacyProjectRuntime;
use lpc_engine::legacy_project::project_runtime::{NodeEntry, NodeStatus};
use lpc_engine::runtime::frame_time::FrameTime;
use lpc_model::AsLpPath;
use lpc_model::{FrameId, LpPathBuf, NodeId};
use lpc_wire::{WireNodeSpecifier, WireNodeStatus as ApiNodeStatus};
use lpfs::{FsChange, LpFs};
use lpl_model::{NodeChange, NodeConfig, NodeDetail, NodeKind, NodeState, ProjectResponse};

pub fn init_nodes(rt: &mut LegacyProjectRuntime) -> Result<(), Error> {
    // Initialize in order: textures → shaders → fixtures → outputs
    let init_order = [
        NodeKind::Texture,
        NodeKind::Shader,
        NodeKind::Fixture,
        NodeKind::Output,
    ];

    for kind in init_order.iter() {
        let handles: Vec<NodeId> = rt
            .nodes
            .iter()
            .filter(|(_, entry)| entry.kind == *kind && entry.status == NodeStatus::Created)
            .map(|(handle, _)| *handle)
            .collect();

        for handle in handles {
            // Get node path and kind before mutable borrow
            let (node_path, node_kind) = {
                let entry = rt.nodes.get(&handle).ok_or_else(|| Error::Other {
                    message: format!("Node handle {} not found", handle.as_u32()),
                })?;
                (entry.path.clone(), entry.kind)
            };

            // Extract config before creating runtime (for textures and fixtures)
            // Load config from filesystem since we can't extract from Box<dyn NodeConfig>
            let texture_config = if node_kind == NodeKind::Texture {
                let entry = rt.nodes.get(&handle).ok_or_else(|| Error::Other {
                    message: format!("Node handle {} not found", handle.as_u32()),
                })?;
                // Reload config from filesystem (workaround for trait object limitation)
                let node_json_path = entry.path.join("node.json");
                let data = rt
                    .fs
                    .borrow()
                    .read_file(node_json_path.as_path())
                    .map_err(|e| Error::Io {
                        path: node_json_path.as_str().to_string(),
                        details: format!("Failed to read: {e:?}"),
                    })?;
                Some(
                    lpc_wire::json::from_slice::<lpl_model::nodes::texture::TextureConfig>(&data)
                        .map_err(|e| Error::Parse {
                        file: node_json_path.as_str().to_string(),
                        error: format!("Failed to parse texture config: {e}"),
                    })?,
                )
            } else {
                None
            };

            let fixture_config = if node_kind == NodeKind::Fixture {
                let entry = rt.nodes.get(&handle).ok_or_else(|| Error::Other {
                    message: format!("Node handle {} not found", handle.as_u32()),
                })?;
                // Reload config from filesystem (workaround for trait object limitation)
                let node_json_path = entry.path.join("node.json");
                let data = rt
                    .fs
                    .borrow()
                    .read_file(node_json_path.as_path())
                    .map_err(|e| Error::Io {
                        path: node_json_path.as_str().to_string(),
                        details: format!("Failed to read: {e:?}"),
                    })?;
                Some(
                    lpc_wire::json::from_slice::<lpl_model::nodes::fixture::FixtureConfig>(&data)
                        .map_err(|e| Error::Parse {
                        file: node_json_path.as_str().to_string(),
                        error: format!("Failed to parse fixture config: {e}"),
                    })?,
                )
            } else {
                None
            };

            let shader_config = if node_kind == NodeKind::Shader {
                let entry = rt.nodes.get(&handle).ok_or_else(|| Error::Other {
                    message: format!("Node handle {} not found", handle.as_u32()),
                })?;
                // Reload config from filesystem (workaround for trait object limitation)
                let node_json_path = entry.path.join("node.json");
                let data = rt
                    .fs
                    .borrow()
                    .read_file(node_json_path.as_path())
                    .map_err(|e| Error::Io {
                        path: node_json_path.as_str().to_string(),
                        details: format!("Failed to read: {e:?}"),
                    })?;
                Some(
                    lpc_wire::json::from_slice::<lpl_model::nodes::shader::ShaderConfig>(&data)
                        .map_err(|e| Error::Parse {
                            file: node_json_path.as_str().to_string(),
                            error: format!("Failed to parse shader config: {e}"),
                        })?,
                )
            } else {
                None
            };

            let output_config = if node_kind == NodeKind::Output {
                let entry = rt.nodes.get(&handle).ok_or_else(|| Error::Other {
                    message: format!("Node handle {} not found", handle.as_u32()),
                })?;
                // Reload config from filesystem (workaround for trait object limitation)
                let node_json_path = entry.path.join("node.json");
                let data = rt
                    .fs
                    .borrow()
                    .read_file(node_json_path.as_path())
                    .map_err(|e| Error::Io {
                        path: node_json_path.as_str().to_string(),
                        details: format!("Failed to read: {e:?}"),
                    })?;
                Some(
                    lpc_wire::json::from_slice::<lpl_model::nodes::output::OutputConfig>(&data)
                        .map_err(|e| Error::Parse {
                            file: node_json_path.as_str().to_string(),
                            error: format!("Failed to parse output config: {e}"),
                        })?,
                )
            } else {
                None
            };

            // Create runtime based on kind
            let mut runtime: Box<dyn LegacyNodeRuntime> = match node_kind {
                NodeKind::Texture => {
                    let mut tex_runtime = TextureRuntime::new(handle);
                    if let Some(config) = texture_config {
                        tex_runtime.set_config(config);
                    }
                    Box::new(tex_runtime)
                }
                NodeKind::Shader => {
                    let mut shader_runtime = ShaderRuntime::new(handle, rt.graphics.clone());
                    if let Some(config) = shader_config {
                        shader_runtime.set_config(config);
                    }
                    Box::new(shader_runtime)
                }
                NodeKind::Output => {
                    let mut output_runtime = OutputRuntime::new();
                    if let Some(config) = output_config {
                        output_runtime.set_config(config);
                    }
                    Box::new(output_runtime)
                }
                NodeKind::Fixture => {
                    let mut fixture_runtime = FixtureRuntime::new();
                    if let Some(config) = fixture_config {
                        fixture_runtime.set_config(config);
                    }
                    Box::new(fixture_runtime)
                }
            };

            // Create init context and initialize (needs immutable borrow of self)
            let init_result = {
                let ctx = InitContext::new(rt, &node_path)?;
                runtime.init(&ctx)
            };

            // Check if this is a shader runtime with compilation error before storing
            // GLSL compilation errors are runtime state errors, not initialization errors
            let shader_compilation_error = if node_kind == NodeKind::Shader {
                // Try to downcast to ShaderRuntime to check compilation error
                runtime
                    .as_any()
                    .downcast_ref::<ShaderRuntime>()
                    .and_then(|sr| sr.compilation_error().map(|s| s.to_string()))
            } else {
                None
            };

            // Now do mutable operations (context is dropped)
            if let Some(entry) = rt.nodes.get_mut(&handle) {
                match init_result {
                    Ok(()) => {
                        if let Some(error_msg) = shader_compilation_error {
                            // Shader initialized but has compilation error - set status to Error
                            entry.status = NodeStatus::Error(error_msg);
                            entry.status_ver = rt.frame_id;
                        } else {
                            // Node initialized successfully
                            entry.status = NodeStatus::Ok;
                            entry.status_ver = rt.frame_id;
                        }
                        entry.runtime = Some(runtime);
                    }
                    Err(e) => {
                        entry.status = NodeStatus::InitError(format!("{e}"));
                        entry.status_ver = rt.frame_id;
                        entry.runtime = None;
                    }
                }
            }
        }
    }

    Ok(())
}
pub fn tick(rt: &mut LegacyProjectRuntime, delta_ms: u32) -> Result<(), Error> {
    lp_perf::emit_begin!(EVENT_FRAME);
    // Update frame ID and time
    let old_frame_id = rt.frame_id;
    rt.frame_id = rt.frame_id.next();
    rt.frame_time.total_ms += delta_ms;
    rt.frame_time.delta_ms = delta_ms;

    log::debug!(
        "ProjectRuntime::tick: Frame {} -> {} (time: {}ms total, {}ms delta)",
        old_frame_id.as_i64(),
        rt.frame_id.as_i64(),
        rt.frame_time.total_ms,
        delta_ms
    );

    // Render the frame
    // Render all fixtures
    let fixture_handles: Vec<NodeId> = rt
        .nodes
        .iter()
        .filter(|(_, entry)| {
            entry.kind == NodeKind::Fixture
                && entry.runtime.is_some()
                && matches!(entry.status, NodeStatus::Ok)
        })
        .map(|(handle, _)| *handle)
        .collect();

    for handle in fixture_handles {
        // Render fixture - need to handle borrowing carefully
        // The issue: runtime.render() needs &mut runtime and &mut ctx
        // But runtime is inside ctx.nodes, so we can't have both borrows
        // Solution: use a helper that takes nodes and handle, does everything internally
        let render_result = catch_node_panic(|| {
            // Create context
            let mut ctx = RenderContextImpl {
                nodes: &mut rt.nodes,
                frame_id: rt.frame_id,
                frame_time: rt.frame_time,
                output_provider: Rc::clone(&rt.output_provider),
            };

            // Get runtime and render in one go
            // We'll use a pattern where we get the runtime, call render, then handle errors
            // The key is that runtime.render() will borrow ctx, and ctx contains nodes
            // So we can't hold a reference to runtime (from nodes) while calling render
            // Solution: restructure so render() accesses runtime internally through ctx
            // But that would require changing the trait signature
            // For now, let's use a workaround: get runtime, call render with reborrow
            if let Some(entry) = ctx.nodes.get_mut(&handle) {
                if let Some(runtime) = entry.runtime.as_mut() {
                    // runtime is &mut Box<dyn NodeRuntime>
                    // render() needs &mut self (runtime) and &mut ctx
                    // Both need mutable access, but runtime is inside ctx.nodes
                    // This creates a borrowing conflict
                    // Workaround: use unsafe to get raw pointer (not ideal, but works)
                    let runtime_ptr: *mut dyn LegacyNodeRuntime = runtime.as_mut();
                    // SAFETY: runtime_ptr is valid for the duration of this block
                    // We're not storing it or using it after the block
                    unsafe { (*runtime_ptr).render(&mut ctx) }
                } else {
                    Ok(())
                }
            } else {
                Ok(())
            }
        });

        // Update status based on render result
        if let Some(entry) = rt.nodes.get_mut(&handle) {
            if let Err(e) = render_result {
                entry.status = NodeStatus::Error(format!("{e}"));
                entry.status_ver = rt.frame_id;
            }
        }
    }

    // Flush outputs with state_ver == frame_id (outputs that were written to this frame)
    let output_handles: Vec<NodeId> = rt
        .nodes
        .iter()
        .filter(|(_, entry)| {
            entry.kind == NodeKind::Output
                && entry.runtime.is_some()
                && entry.state_ver == rt.frame_id
                && matches!(entry.status, NodeStatus::Ok)
        })
        .map(|(handle, _)| *handle)
        .collect();

    for handle in output_handles {
        let render_result = catch_node_panic(|| {
            let mut ctx = RenderContextImpl {
                nodes: &mut rt.nodes,
                frame_id: rt.frame_id,
                frame_time: rt.frame_time,
                output_provider: Rc::clone(&rt.output_provider),
            };

            if let Some(entry) = ctx.nodes.get_mut(&handle) {
                if let Some(runtime) = entry.runtime.as_mut() {
                    let runtime_ptr: *mut dyn LegacyNodeRuntime = runtime.as_mut();
                    unsafe { (*runtime_ptr).render(&mut ctx) }
                } else {
                    Ok(())
                }
            } else {
                Ok(())
            }
        });

        if let Err(e) = render_result {
            if let Some(entry) = rt.nodes.get_mut(&handle) {
                entry.status = NodeStatus::Error(format!("{e}"));
                entry.status_ver = rt.frame_id;
            }
        }
    }

    lp_perf::emit_end!(EVENT_FRAME);
    Ok(())
}

/// Handle filesystem changes
///
/// Processes filesystem change events and updates affected nodes.
/// Should be called before tick() when filesystem changes occur.
pub fn handle_fs_changes(rt: &mut LegacyProjectRuntime, changes: &[FsChange]) -> Result<(), Error> {
    // Process deletions first
    for change in changes {
        if matches!(change.change_type, lpfs::ChangeType::Delete) {
            rt.handle_delete_change(change)?;
        }
    }

    // Process creates (new node directories)
    for change in changes {
        if matches!(change.change_type, lpfs::ChangeType::Create) {
            rt.handle_create_change(change)?;
        }
    }

    // Process modifies (existing files)
    for change in changes {
        if matches!(change.change_type, lpfs::ChangeType::Modify) {
            handle_modify_change(rt, change)?;
        }
    }

    Ok(())
}

/// Handle a modify change
fn handle_modify_change(rt: &mut LegacyProjectRuntime, change: &FsChange) -> Result<(), Error> {
    // Find which node this file belongs to - collect handle, path, and kind
    let mut target_handle: Option<NodeId> = None;
    let mut target_path: Option<LpPathBuf> = None;
    let mut target_kind: Option<NodeKind> = None;

    for (handle, entry) in &rt.nodes {
        if rt.file_belongs_to_node(change.path.as_path(), entry.path.as_path()) {
            target_handle = Some(*handle);
            target_path = Some(entry.path.clone());
            target_kind = Some(entry.kind);
            break;
        }
    }

    if let (Some(handle), Some(path), Some(kind)) = (target_handle, target_path, target_kind) {
        // Check if it's node.json
        if change.path.has_suffix("/node.json") {
            // Reload config
            let (_, config_for_update) =
                lpc_engine::legacy_project::legacy_loader::legacy_load_node(&*rt.fs.borrow(), &path)?;
            let (_, new_config) =
                lpc_engine::legacy_project::legacy_loader::legacy_load_node(&*rt.fs.borrow(), &path)?;

            // Update node entry config
            let has_runtime = {
                if let Some(node_entry) = rt.nodes.get_mut(&handle) {
                    node_entry.config = new_config;
                    node_entry.config_ver = rt.frame_id;
                    node_entry.runtime.is_some()
                } else {
                    false
                }
            };

            // Call update_config on runtime if it exists
            if has_runtime {
                // Extract runtime first to avoid borrow conflicts
                let mut runtime_opt = None;
                if let Some(node_entry) = rt.nodes.get_mut(&handle) {
                    runtime_opt = node_entry.runtime.take();
                }

                if let Some(mut runtime) = runtime_opt {
                    let ctx = InitContext::new(rt, &path)?;
                    runtime.update_config(config_for_update, &ctx)?;
                    // Put runtime back
                    if let Some(node_entry) = rt.nodes.get_mut(&handle) {
                        node_entry.runtime = Some(runtime);
                    }
                }
            }
            log::info!("Node {} [{:?}] config reloaded", path.as_str(), kind);
        } else {
            // Other file change - call handle_fs_change on the node runtime
            // Convert full path to relative path (node directory is chrooted in InitContext)
            let relative_path = if let Some(stripped) = change.path.strip_prefix(path.as_str()) {
                // strip_prefix returns a path starting with "/" if the result is absolute
                // We want the relative part without the leading slash
                let stripped_str = stripped.as_str();
                if stripped_str == "/" {
                    ""
                } else if let Some(stripped) = stripped_str.strip_prefix('/') {
                    stripped
                } else {
                    stripped_str
                }
            } else {
                // Fallback: use full path if it doesn't match (shouldn't happen)
                change.path.as_str()
            };

            // Shed optional buffers before shader recompile to maximize memory
            let is_shader_glsl = change.path.has_suffix(".glsl")
                && rt
                    .nodes
                    .get(&handle)
                    .map(|e| e.kind == NodeKind::Shader)
                    .unwrap_or(false);

            if is_shader_glsl {
                let free_before = rt.memory_stats.and_then(|f| f().map(|(free, _)| free));
                if let Some(free) = free_before {
                    log::info!("[mem] shed_before_shader_recompile: {}k free", free / 1024);
                }

                let provider = rt.output_provider.borrow();
                let output_provider: &dyn OutputProvider = &*provider;
                for (_, entry) in &mut rt.nodes {
                    if let Some(runtime) = entry.runtime.as_mut() {
                        runtime.shed_optional_buffers(Some(output_provider))?;
                    }
                }
                drop(provider);

                if let Some(f) = rt.memory_stats {
                    if let Some((free_after, _)) = f() {
                        let delta = free_after as i64 - free_before.unwrap_or(0) as i64;
                        let sign = if delta >= 0 { "+" } else { "" };
                        log::info!(
                            "[mem] shed_after_shader_recompile: {}k free (delta: {}{}k)",
                            free_after / 1024,
                            sign,
                            delta / 1024
                        );
                    }
                }
            }

            // Create FsChange with relative path
            let relative_change = FsChange {
                path: LpPathBuf::from(relative_path),
                change_type: change.change_type,
            };

            log::info!(
                "Node file changed: {} -> {} (handle_fs_change)",
                path.as_str(),
                relative_path
            );

            let mut runtime_opt = None;
            if let Some(node_entry) = rt.nodes.get_mut(&handle) {
                runtime_opt = node_entry.runtime.take();
            }

            if let Some(mut runtime) = runtime_opt {
                let ctx = InitContext::new(rt, &path)?;
                // handle_fs_change now returns Ok() even on compilation errors
                runtime.handle_fs_change(&relative_change, &ctx)?;
                // Drop context before mutating nodes
                drop(ctx);

                // Check if this is a shader runtime with compilation error
                let shader_compilation_error = runtime
                    .as_any()
                    .downcast_ref::<ShaderRuntime>()
                    .and_then(|sr| sr.compilation_error().map(|s| s.to_string()));

                // Put runtime back and update status
                if let Some(node_entry) = rt.nodes.get_mut(&handle) {
                    let old_status = node_entry.status.clone();
                    node_entry.runtime = Some(runtime);

                    // Update status based on compilation error state
                    if let Some(error_msg) = shader_compilation_error {
                        // Shader has compilation error - update status to Error
                        node_entry.status = NodeStatus::Error(error_msg);
                        node_entry.status_ver = rt.frame_id;
                    } else if matches!(old_status, NodeStatus::Error(_)) {
                        // No compilation error and status was Error - update to Ok
                        node_entry.status = NodeStatus::Ok;
                        node_entry.status_ver = rt.frame_id;
                    }
                    // Status change will be picked up in get_changes() if status changed
                }
            }
        }
    }

    Ok(())
}
pub fn get_changes(
    rt: &LegacyProjectRuntime,
    since_frame: FrameId,
    detail_specifier: &WireNodeSpecifier,
    theoretical_fps: Option<f32>,
) -> Result<ProjectResponse, Error> {
    let mut node_handles = Vec::new();
    let mut node_changes = Vec::new();
    let mut node_details = BTreeMap::new();

    // Collect all current handles
    for handle in rt.nodes.keys() {
        node_handles.push(*handle);
    }

    // Determine which handles need detail
    let detail_handles: BTreeSet<NodeId> = match detail_specifier {
        WireNodeSpecifier::None => BTreeSet::new(),
        WireNodeSpecifier::All => rt.nodes.keys().copied().collect(),
        WireNodeSpecifier::ByHandles(handles) => handles.iter().copied().collect(),
    };

    // Collect changes and details
    for (handle, entry) in &rt.nodes {
        // Emit `Created` before `StatusChanged` so clients that apply changes in order create
        // the entry first; otherwise `StatusChanged` is a no-op and `Created` leaves
        // `NodeStatus::Created` (see `ProjectView::apply_changes`).
        if entry.config_ver.as_i64() > since_frame.as_i64() && entry.config_ver == entry.state_ver {
            node_changes.push(NodeChange::Created {
                handle: *handle,
                path: entry.path.clone(),
                kind: entry.kind,
            });
        }

        // Check for changes since since_frame
        if entry.config_ver.as_i64() > since_frame.as_i64() {
            node_changes.push(NodeChange::ConfigUpdated {
                handle: *handle,
                config_ver: entry.config_ver,
            });
        }

        if entry.state_ver.as_i64() > since_frame.as_i64() {
            node_changes.push(NodeChange::StateUpdated {
                handle: *handle,
                state_ver: entry.state_ver,
            });
        }

        // Always send current status for all nodes (requirement: always send status even for unwatched nodes)
        // If status changed since since_frame, send StatusChanged
        // Otherwise, still send current status so client has it
        let api_status = match &entry.status {
            NodeStatus::Created => ApiNodeStatus::Created,
            NodeStatus::InitError(msg) => ApiNodeStatus::InitError(msg.clone()),
            NodeStatus::Ok => ApiNodeStatus::Ok,
            NodeStatus::Warn(msg) => ApiNodeStatus::Warn(msg.clone()),
            NodeStatus::Error(msg) => ApiNodeStatus::Error(msg.clone()),
        };

        // Always include status - if it changed since since_frame, or if this is the first sync (since_frame is default)
        // For first sync (since_frame == 0), we want to send status for all nodes
        // For subsequent syncs, we only send if status changed
        if entry.status_ver.as_i64() > since_frame.as_i64() || since_frame == FrameId::default() {
            node_changes.push(NodeChange::StatusChanged {
                handle: *handle,
                status: api_status,
            });
        }

        // Add detail if requested
        if detail_handles.contains(handle) {
            let state = match entry.kind {
                NodeKind::Texture => {
                    // Get actual texture state from runtime
                    if let Some(runtime) = &entry.runtime {
                        // Use Any trait for downcasting (downcast_ref is from Any trait)
                        if let Some(tex_runtime) = runtime.as_any().downcast_ref::<TextureRuntime>()
                        {
                            // Clone state and update with current texture data
                            let state = tex_runtime.state.clone();
                            // TODO(M4a): texture_data / live pixels should come from upstream shader buffer
                            NodeState::Texture(state)
                        } else {
                            // Fallback to empty state
                            NodeState::Texture(lpl_model::nodes::texture::TextureState::new(
                                rt.frame_id,
                            ))
                        }
                    } else {
                        NodeState::Texture(lpl_model::nodes::texture::TextureState::new(
                            rt.frame_id,
                        ))
                    }
                }
                NodeKind::Shader => {
                    // Get actual shader state from runtime
                    if let Some(runtime) = &entry.runtime {
                        if let Some(shader_runtime) =
                            runtime.as_any().downcast_ref::<ShaderRuntime>()
                        {
                            NodeState::Shader(shader_runtime.state.clone())
                        } else {
                            // Fallback to empty state
                            NodeState::Shader(lpl_model::nodes::shader::ShaderState::new(
                                rt.frame_id,
                            ))
                        }
                    } else {
                        NodeState::Shader(lpl_model::nodes::shader::ShaderState::new(rt.frame_id))
                    }
                }
                NodeKind::Output => {
                    // Get actual output state from runtime
                    if let Some(runtime) = &entry.runtime {
                        if let Some(output_runtime) = runtime
                            .as_any()
                            .downcast_ref::<crate::nodes::OutputRuntime>()
                        {
                            // Clone state and update with current channel data
                            let mut state = output_runtime.state.clone();
                            // Update channel_data from current buffer
                            state
                                .channel_data
                                .set(rt.frame_id, output_runtime.get_channel_data());
                            NodeState::Output(state)
                        } else {
                            NodeState::Output(lpl_model::nodes::output::OutputState::new(
                                rt.frame_id,
                            ))
                        }
                    } else {
                        NodeState::Output(lpl_model::nodes::output::OutputState::new(rt.frame_id))
                    }
                }
                NodeKind::Fixture => {
                    // Fixture runtime state extraction - just clone the state directly
                    if let Some(runtime) = &entry.runtime {
                        if let Some(fixture_runtime) =
                            runtime.as_any().downcast_ref::<FixtureRuntime>()
                        {
                            NodeState::Fixture(fixture_runtime.state.clone())
                        } else {
                            // Fallback to empty state
                            NodeState::Fixture(lpl_model::nodes::fixture::FixtureState::new(
                                rt.frame_id,
                            ))
                        }
                    } else {
                        NodeState::Fixture(lpl_model::nodes::fixture::FixtureState::new(
                            rt.frame_id,
                        ))
                    }
                }
            };

            // Clone config based on kind - extract from runtime if available
            let config: Box<dyn NodeConfig> = match entry.kind {
                NodeKind::Texture => {
                    if let Some(runtime) = &entry.runtime {
                        if let Some(tex_runtime) = runtime.as_any().downcast_ref::<TextureRuntime>()
                        {
                            if let Some(tex_config) = tex_runtime.get_config() {
                                Box::new(tex_config.clone())
                            } else {
                                Box::new(lpl_model::nodes::texture::TextureConfig {
                                    width: 0,
                                    height: 0,
                                })
                            }
                        } else {
                            Box::new(lpl_model::nodes::texture::TextureConfig {
                                width: 0,
                                height: 0,
                            })
                        }
                    } else {
                        Box::new(lpl_model::nodes::texture::TextureConfig {
                            width: 0,
                            height: 0,
                        })
                    }
                }
                NodeKind::Shader => {
                    if let Some(runtime) = &entry.runtime {
                        if let Some(shader_runtime) =
                            runtime.as_any().downcast_ref::<ShaderRuntime>()
                        {
                            if let Some(shader_config) = shader_runtime.get_config() {
                                Box::new(shader_config.clone())
                            } else {
                                Box::new(lpl_model::nodes::shader::ShaderConfig::default())
                            }
                        } else {
                            Box::new(lpl_model::nodes::shader::ShaderConfig::default())
                        }
                    } else {
                        Box::new(lpl_model::nodes::shader::ShaderConfig::default())
                    }
                }
                NodeKind::Output => {
                    if let Some(runtime) = &entry.runtime {
                        if let Some(output_runtime) =
                            runtime.as_any().downcast_ref::<OutputRuntime>()
                        {
                            if let Some(output_config) = output_runtime.get_config() {
                                Box::new(output_config.clone())
                            } else {
                                Box::new(lpl_model::nodes::output::OutputConfig::GpioStrip {
                                    pin: 0,
                                    options: None,
                                })
                            }
                        } else {
                            Box::new(lpl_model::nodes::output::OutputConfig::GpioStrip {
                                pin: 0,
                                options: None,
                            })
                        }
                    } else {
                        Box::new(lpl_model::nodes::output::OutputConfig::GpioStrip {
                            pin: 0,
                            options: None,
                        })
                    }
                }
                NodeKind::Fixture => {
                    if let Some(runtime) = &entry.runtime {
                        if let Some(fixture_runtime) =
                            runtime.as_any().downcast_ref::<FixtureRuntime>()
                        {
                            if let Some(fixture_config) = fixture_runtime.get_config() {
                                Box::new(fixture_config.clone())
                            } else {
                                Box::new(lpl_model::nodes::fixture::FixtureConfig {
                                    output_spec: lpc_model::NodeSpec::from(""),
                                    texture_spec: lpc_model::NodeSpec::from(""),
                                    mapping: lpl_model::nodes::fixture::MappingConfig::PathPoints {
                                        paths: vec![],
                                        sample_diameter: 2.0,
                                    },
                                    color_order: lpl_model::nodes::fixture::ColorOrder::Rgb,
                                    transform: [[0.0; 4]; 4],
                                    brightness: None,
                                    gamma_correction: None,
                                })
                            }
                        } else {
                            Box::new(lpl_model::nodes::fixture::FixtureConfig {
                                output_spec: lpc_model::NodeSpec::from(""),
                                texture_spec: lpc_model::NodeSpec::from(""),
                                mapping: lpl_model::nodes::fixture::MappingConfig::PathPoints {
                                    paths: vec![],
                                    sample_diameter: 2.0,
                                },
                                color_order: lpl_model::nodes::fixture::ColorOrder::Rgb,
                                transform: [[0.0; 4]; 4],
                                brightness: None,
                                gamma_correction: None,
                            })
                        }
                    } else {
                        Box::new(lpl_model::nodes::fixture::FixtureConfig {
                            output_spec: lpc_model::NodeSpec::from(""),
                            texture_spec: lpc_model::NodeSpec::from(""),
                            mapping: lpl_model::nodes::fixture::MappingConfig::PathPoints {
                                paths: vec![],
                                sample_diameter: 2.0,
                            },
                            color_order: lpl_model::nodes::fixture::ColorOrder::Rgb,
                            transform: [[0.0; 4]; 4],
                            brightness: None,
                            gamma_correction: None,
                        })
                    }
                }
            };

            node_details.insert(
                *handle,
                NodeDetail {
                    path: entry.path.clone(),
                    config,
                    state,
                },
            );
        }
    }

    Ok(ProjectResponse::GetChanges {
        current_frame: rt.frame_id,
        since_frame,
        node_handles,
        node_changes,
        node_details,
        theoretical_fps,
    })
}

struct InitContext<'a> {
    runtime: &'a lpc_engine::legacy_project::LegacyProjectRuntime,
    #[allow(
        dead_code,
        reason = "Used for chroot filesystem creation, may be needed for future features"
    )]
    node_path: &'a LpPathBuf,
    node_fs: alloc::rc::Rc<core::cell::RefCell<dyn LpFs>>,
}

impl<'a> InitContext<'a> {
    pub fn new(
        runtime: &'a lpc_engine::legacy_project::LegacyProjectRuntime,
        node_path: &'a LpPathBuf,
    ) -> Result<Self, Error> {
        let node_dir = node_path.as_str();
        let node_fs = runtime
            .fs
            .borrow()
            .chroot(node_dir.as_path())
            .map_err(|e| Error::Io {
                path: node_dir.to_string(),
                details: format!("Failed to chroot: {e:?}"),
            })?;

        Ok(Self {
            runtime,
            node_path,
            node_fs,
        })
    }
}

impl<'a> lpc_engine::runtime::contexts::NodeInitContext for InitContext<'a> {
    fn resolve_node(&self, spec: &lpc_model::NodeSpec) -> Result<lpc_model::NodeId, Error> {
        let spec_path = spec.as_str();
        let node_path = if spec_path.starts_with('/') {
            // Absolute path
            lpc_model::LpPathBuf::from(spec_path)
        } else {
            // Relative path - resolve from current node's directory
            // Current node path is self.node_path (e.g., "/src/texture.texture")
            // Relative spec is relative to the parent directory (e.g., "../output.output")
            let parent_dir = self
                .node_path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| {
                    // No parent, use root
                    lpc_model::LpPathBuf::from("/")
                });

            // Resolve relative path using join_relative
            parent_dir
                .join_relative(spec_path)
                .ok_or_else(|| Error::InvalidConfig {
                    node_path: spec_path.to_string(),
                    reason: "Invalid relative path resolution".to_string(),
                })?
        };

        // Look up node by path
        for (handle, entry) in &self.runtime.nodes {
            if entry.path == node_path {
                return Ok(*handle);
            }
        }

        Err(Error::NotFound {
            path: spec_path.to_string(),
        })
    }

    fn resolve_output(
        &self,
        spec: &lpc_model::NodeSpec,
    ) -> Result<lpc_engine::runtime::contexts::OutputHandle, Error> {
        let handle = self.resolve_node(spec)?;
        let entry = self
            .runtime
            .nodes
            .get(&handle)
            .ok_or_else(|| Error::NotFound {
                path: spec.as_str().to_string(),
            })?;

        if entry.kind != lpl_model::NodeKind::Output {
            return Err(Error::WrongNodeKind {
                specifier: spec.as_str().to_string(),
                expected: lpl_model::NodeKind::Output,
                actual: entry.kind,
            });
        }

        Ok(lpc_engine::runtime::contexts::OutputHandle::new(handle))
    }

    fn resolve_texture(
        &self,
        spec: &lpc_model::NodeSpec,
    ) -> Result<lpc_engine::runtime::contexts::TextureHandle, Error> {
        let handle = self.resolve_node(spec)?;
        let entry = self
            .runtime
            .nodes
            .get(&handle)
            .ok_or_else(|| Error::NotFound {
                path: spec.as_str().to_string(),
            })?;

        if entry.kind != lpl_model::NodeKind::Texture {
            return Err(Error::WrongNodeKind {
                specifier: spec.as_str().to_string(),
                expected: lpl_model::NodeKind::Texture,
                actual: entry.kind,
            });
        }

        Ok(lpc_engine::runtime::contexts::TextureHandle::new(handle))
    }

    fn get_node_fs(&self) -> &dyn lpfs::LpFs {
        // SAFETY: We're returning a reference from a RefCell borrow, but the trait only allows
        // immutable access and we're not holding the borrow across any potential panics.
        // The borrow is valid for the lifetime of the returned reference.
        unsafe { &*self.node_fs.as_ptr() }
    }

    fn output_provider(&self) -> &dyn OutputProvider {
        // We can't return a reference from RefCell borrow, so we need to use unsafe
        // SAFETY: This is safe because the trait only allows immutable access
        // and we're not holding the borrow across any potential panics
        unsafe { &*self.runtime.output_provider.as_ptr() }
    }

    fn now_ms(&self) -> Option<u64> {
        self.runtime.time_provider.as_ref().map(|t| t.now_ms())
    }

    fn get_texture_config(
        &self,
        handle: lpc_engine::runtime::contexts::TextureHandle,
    ) -> Result<lpl_model::nodes::texture::TextureConfig, Error> {
        let node_handle = handle.as_node_handle();
        let entry = self
            .runtime
            .nodes
            .get(&node_handle)
            .ok_or_else(|| Error::NotFound {
                path: format!("texture-{}", node_handle.as_u32()),
            })?;

        if entry.kind != NodeKind::Texture {
            return Err(Error::WrongNodeKind {
                specifier: format!("texture-{}", node_handle.as_u32()),
                expected: NodeKind::Texture,
                actual: entry.kind,
            });
        }

        let runtime = entry.runtime.as_ref().ok_or_else(|| Error::Other {
            message: format!("Texture node {} has no runtime", node_handle.as_u32()),
        })?;

        let tex_runtime = runtime
            .as_any()
            .downcast_ref::<TextureRuntime>()
            .ok_or_else(|| Error::Other {
                message: format!("Node {} is not a texture runtime", node_handle.as_u32()),
            })?;

        tex_runtime
            .get_config()
            .cloned()
            .ok_or_else(|| Error::InvalidConfig {
                node_path: format!("texture-{}", node_handle.as_u32()),
                reason: String::from("Config not set"),
            })
    }

    fn texture_output_buffer_owner(
        &self,
        handle: lpc_engine::runtime::contexts::TextureHandle,
        fallback_if_no_shader_yet: NodeId,
    ) -> NodeId {
        texture_output_owner_handle(&self.runtime.nodes, handle)
            .unwrap_or(fallback_if_no_shader_yet)
    }
}

/// Shader node that owns the shared CPU buffer for a texture target (see [`texture_output_owner_handle`]).
fn texture_output_owner_handle(
    nodes: &BTreeMap<lpc_model::NodeId, lpc_engine::legacy_project::project_runtime::NodeEntry>,
    texture_handle: lpc_engine::runtime::contexts::TextureHandle,
) -> Option<NodeId> {
    let mut best: Option<(i32, NodeId)> = None;
    for (h, entry) in nodes.iter() {
        if entry.kind != NodeKind::Shader || entry.status != NodeStatus::Ok {
            continue;
        }
        let Some(runtime) = entry.runtime.as_ref() else {
            continue;
        };
        let Some(shader) = runtime.as_any().downcast_ref::<ShaderRuntime>() else {
            continue;
        };
        if !shader.targets_texture(texture_handle) {
            continue;
        }
        let ord = shader.render_order();
        let replace = match best {
            None => true,
            Some((best_ord, best_h)) => {
                ord > best_ord || (ord == best_ord && h.as_u32() > best_h.as_u32())
            }
        };
        if replace {
            best = Some((ord, *h));
        }
    }
    best.map(|(_, h)| h)
}

/// Render context implementation
struct RenderContextImpl<'a> {
    nodes: &'a mut BTreeMap<NodeId, NodeEntry>,
    frame_id: FrameId,
    frame_time: FrameTime,
    output_provider: Rc<RefCell<dyn OutputProvider>>,
}

impl<'a> lpc_engine::runtime::contexts::RenderContext for RenderContextImpl<'a> {
    fn get_texture(
        &mut self,
        handle: lpc_engine::runtime::contexts::TextureHandle,
    ) -> Result<&dyn lps_shared::TextureBuffer, Error> {
        Self::ensure_texture_rendered(
            self.nodes,
            handle,
            self.frame_id,
            self.frame_time,
            Rc::clone(&self.output_provider),
        )?;

        let shader = Self::find_shader_for_texture(self.nodes, handle).ok_or_else(|| Error::Other {
            message: format!(
                "Texture {} has no shader writing to it; there are no shader-independent texture sources",
                handle.as_node_handle().as_u32()
            ),
        })?;

        shader.output_buffer().ok_or_else(|| Error::Other {
            message: format!(
                "Shader output buffer for texture {} is not allocated",
                handle.as_node_handle().as_u32()
            ),
        })
    }

    fn get_target_texture_pixels_mut(
        &mut self,
        handle: lpc_engine::runtime::contexts::TextureHandle,
    ) -> Result<&mut lp_shader::LpsTextureBuf, Error> {
        Self::ensure_texture_rendered(
            self.nodes,
            handle,
            self.frame_id,
            self.frame_time,
            Rc::clone(&self.output_provider),
        )?;

        let shader = Self::find_shader_for_texture(self.nodes, handle).ok_or_else(|| Error::Other {
            message: format!(
                "Texture {} has no shader writing to it; there are no shader-independent texture sources",
                handle.as_node_handle().as_u32()
            ),
        })?;

        shader.output_buffer_mut().ok_or_else(|| Error::Other {
            message: format!(
                "Shader output buffer for texture {} is not allocated",
                handle.as_node_handle().as_u32()
            ),
        })
    }

    fn get_time(&self) -> f32 {
        // Convert total_ms to seconds
        self.frame_time.total_ms as f32 / 1000.0
    }

    fn get_output(
        &mut self,
        handle: lpc_engine::runtime::contexts::OutputHandle,
        _universe: u32,
        start_ch: u32,
        ch_count: u32,
    ) -> Result<&mut [u16], Error> {
        // Get output runtime
        let node_handle = handle.as_node_handle();
        let entry = self
            .nodes
            .get_mut(&node_handle)
            .ok_or_else(|| Error::NotFound {
                path: format!("output-{}", node_handle.as_u32()),
            })?;

        // Update output state_ver to current frame (state changed when accessed)
        let old_state_ver = entry.state_ver;
        entry.state_ver = self.frame_id;
        if old_state_ver != self.frame_id {
            log::debug!(
                "RenderContext::get_output: Output {} ({}) state_ver updated: {} -> {} (channels {}-{})",
                node_handle.as_u32(),
                entry.path.as_str(),
                old_state_ver.as_i64(),
                self.frame_id.as_i64(),
                start_ch,
                start_ch + ch_count - 1
            );
        }

        // Get output buffer from runtime
        if let Some(runtime) = &mut entry.runtime {
            if let Some(output_runtime) = runtime
                .as_any_mut()
                .downcast_mut::<crate::nodes::OutputRuntime>()
            {
                Ok(output_runtime.get_buffer_mut(start_ch, ch_count))
            } else {
                Err(Error::Other {
                    message: "Output runtime not found".to_string(),
                })
            }
        } else {
            Err(Error::Other {
                message: "Runtime not initialized".to_string(),
            })
        }
    }

    fn output_provider(&self) -> &dyn OutputProvider {
        // We can't return a reference from RefCell borrow, so we need to use unsafe
        // SAFETY: This is safe because the trait only allows immutable access
        // and we're not holding the borrow across any potential panics
        unsafe { &*self.output_provider.as_ptr() }
    }

    fn frame_id(&self) -> FrameId {
        self.frame_id
    }
}

impl<'a> RenderContextImpl<'a> {
    /// Ensure texture is rendered for current frame (lazy rendering)
    ///
    /// This function:
    /// 1. Finds all shader nodes that target this texture
    /// 2. Renders those shaders in render_order (lowest first)
    /// 3. Marks the texture as rendered
    fn ensure_texture_rendered(
        nodes: &mut BTreeMap<NodeId, NodeEntry>,
        handle: lpc_engine::runtime::contexts::TextureHandle,
        frame_id: FrameId,
        frame_time: FrameTime,
        output_provider: Rc<RefCell<dyn OutputProvider>>,
    ) -> Result<(), Error> {
        let node_handle = handle.as_node_handle();

        log::trace!(
            "RenderContextImpl::ensure_texture_rendered: Ensuring texture {} is rendered (frame {})",
            node_handle.as_u32(),
            frame_id.as_i64()
        );

        // Check if already rendered
        if let Some(entry) = nodes.get(&node_handle) {
            if entry.state_ver >= frame_id {
                log::trace!(
                    "RenderContextImpl::ensure_texture_rendered: Texture {} already rendered",
                    node_handle.as_u32()
                );
                return Ok(());
            }
        }

        // Find all shader nodes that target this texture
        // Collect (handle, render_order) pairs for shaders targeting this texture
        let mut shader_handles: Vec<(NodeId, i32)> = Vec::new();

        for (shader_handle, entry) in nodes.iter() {
            if entry.kind == NodeKind::Shader
                && entry.status == NodeStatus::Ok
                && entry.runtime.is_some()
            {
                // Check if this shader targets our texture
                if let Some(runtime) = entry.runtime.as_ref() {
                    if let Some(shader_runtime) = runtime
                        .as_any()
                        .downcast_ref::<crate::nodes::ShaderRuntime>()
                    {
                        if shader_runtime.targets_texture(handle) {
                            // Get render_order from shader runtime
                            let render_order = shader_runtime.render_order();
                            shader_handles.push((*shader_handle, render_order));
                        }
                    }
                }
            }
        }

        // Sort by render_order (lowest first)
        shader_handles.sort_by_key(|(_, order)| *order);

        log::trace!(
            "RenderContextImpl::ensure_texture_rendered: Found {} shader(s) targeting texture {}",
            shader_handles.len(),
            node_handle.as_u32()
        );

        // Mark texture as rendering BEFORE calling shader.render() to prevent infinite recursion
        // when the shader samples another texture via get_texture().
        if let Some(entry) = nodes.get_mut(&node_handle) {
            entry.state_ver = frame_id;
        }

        // Render each shader that targets this texture
        for (shader_handle, _) in shader_handles {
            log::trace!(
                "RenderContextImpl::ensure_texture_rendered: Rendering shader {} for texture {}",
                shader_handle.as_u32(),
                node_handle.as_u32()
            );
            // Create RenderContext for each shader render
            let mut ctx = RenderContextImpl {
                nodes,
                frame_id,
                frame_time,
                output_provider: Rc::clone(&output_provider),
            };

            // Get shader runtime and render
            // Use unsafe to work around borrow checker (same pattern as fixture rendering)
            let render_result = catch_node_panic(|| {
                if let Some(entry) = ctx.nodes.get_mut(&shader_handle) {
                    if let Some(runtime) = entry.runtime.as_mut() {
                        // runtime is &mut Box<dyn NodeRuntime>
                        // render() needs &mut self (runtime) and &mut ctx
                        // Both need mutable access, but runtime is inside ctx.nodes
                        // Workaround: use unsafe to get raw pointer
                        let runtime_ptr: *mut dyn LegacyNodeRuntime = runtime.as_mut();
                        // SAFETY: runtime_ptr is valid for the duration of this block
                        // We're not storing it or using it after the block
                        unsafe { (*runtime_ptr).render(&mut ctx) }
                    } else {
                        Ok(())
                    }
                } else {
                    Ok(())
                }
            });

            // Handle render errors - if shader execution fails, update shader status
            match render_result {
                Ok(()) => {
                    // Update shader state_ver after successful render
                    if let Some(entry) = nodes.get_mut(&shader_handle) {
                        entry.state_ver = frame_id;
                    }
                }
                Err(e) => {
                    // Check if this is a shader execution error
                    let error_msg = format!("{e}");
                    if error_msg.contains("Shader execution failed") {
                        // Update shader status to Error
                        if let Some(entry) = nodes.get_mut(&shader_handle) {
                            entry.status = NodeStatus::Error(error_msg.clone());
                            entry.status_ver = frame_id;
                        }
                        // Don't propagate error - shader already has error status
                        // This prevents fixture/texture from getting the error
                    } else {
                        // Other errors (e.g., texture not found) should propagate
                        return Err(e);
                    }
                }
            }
        }

        Ok(())
    }

    /// Resolves the shader that owns the shared output buffer for this texture (see
    /// [`texture_output_owner_handle`]).
    fn find_shader_for_texture(
        nodes: &mut BTreeMap<NodeId, NodeEntry>,
        texture_handle: lpc_engine::runtime::contexts::TextureHandle,
    ) -> Option<&mut ShaderRuntime> {
        let best_h = texture_output_owner_handle(nodes, texture_handle)?;
        let entry = nodes.get_mut(&best_h)?;
        let runtime = entry.runtime.as_mut()?;
        runtime.as_any_mut().downcast_mut::<ShaderRuntime>()
    }
}
