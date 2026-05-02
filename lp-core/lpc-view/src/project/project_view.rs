use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::String;
use alloc::{vec, vec::Vec};
use lpc_model::{FrameId, LpPathBuf, NodeId};
use lpc_source::legacy::nodes::fixture::FixtureConfig;
use lpc_source::legacy::nodes::output::OutputConfig;
use lpc_source::legacy::nodes::shader::ShaderConfig;
use lpc_source::legacy::nodes::texture::TextureConfig;
use lpc_source::legacy::nodes::{NodeConfig, NodeKind};
use lpc_wire::legacy::{NodeChange, NodeState, ProjectResponse};
use lpc_wire::{WireNodeSpecifier, WireNodeStatus};

use super::resource_cache::{self, ClientResourceCache};

/// Status change information surfaced by [`ProjectView::apply_changes`].
#[derive(Debug, Clone)]
pub struct StatusChangeView {
    /// Node path
    pub path: LpPathBuf,
    /// Previous status
    pub old_status: WireNodeStatus,
    /// New status
    pub new_status: WireNodeStatus,
}

/// Cached view of project state synced from engine responses.
pub struct ProjectView {
    /// Current frame ID (last synced)
    pub frame_id: FrameId,
    /// Node entries
    pub nodes: BTreeMap<NodeId, NodeEntryView>,
    /// Which nodes we're tracking detail for
    pub detail_tracking: BTreeSet<NodeId>,
    /// Previous status for each node (for detecting status changes)
    previous_status: BTreeMap<NodeId, WireNodeStatus>,
    /// Cached resource summaries and payloads from `GetChanges`.
    pub resource_cache: ClientResourceCache,
}

pub struct NodeEntryView {
    pub path: LpPathBuf,
    pub kind: NodeKind,
    pub config: Box<dyn NodeConfig>,
    pub config_ver: FrameId,
    pub state: Option<NodeState>, // Only present if in detail_tracking
    pub state_ver: FrameId,
    pub status: WireNodeStatus,
    pub status_ver: FrameId,
}

impl ProjectView {
    /// Create new client view
    pub fn new() -> Self {
        Self {
            frame_id: FrameId::default(),
            nodes: BTreeMap::new(),
            detail_tracking: BTreeSet::new(),
            previous_status: BTreeMap::new(),
            resource_cache: ClientResourceCache::new(),
        }
    }

    /// Start tracking detail for a node
    pub fn watch_detail(&mut self, handle: NodeId) {
        self.detail_tracking.insert(handle);
    }

    /// Stop tracking detail for a node
    pub fn unwatch_detail(&mut self, handle: NodeId) {
        self.detail_tracking.remove(&handle);
        // Clear state when stopping detail
        if let Some(entry) = self.nodes.get_mut(&handle) {
            entry.state = None;
        }
    }

    /// Generate detail specifier for sync
    pub fn detail_specifier(&self) -> WireNodeSpecifier {
        if self.detail_tracking.is_empty() {
            WireNodeSpecifier::None
        } else {
            WireNodeSpecifier::ByHandles(self.detail_tracking.iter().copied().collect())
        }
    }

    /// Sync with server (update view from response)
    ///
    /// Returns a list of all status changes that the caller can use for logging or other purposes.
    pub fn apply_changes(
        &mut self,
        response: &ProjectResponse,
    ) -> Result<Vec<StatusChangeView>, String> {
        let mut status_changes = Vec::new();
        match response {
            ProjectResponse::GetChanges {
                current_frame,
                since_frame: _,
                node_handles,
                node_changes,
                node_details,
                theoretical_fps: _,
                resource_summaries,
                runtime_buffer_payloads,
                render_product_payloads,
            } => {
                // Update frame ID
                self.frame_id = *current_frame;

                self.resource_cache.apply_summaries(resource_summaries);
                self.resource_cache
                    .apply_runtime_buffer_payloads(runtime_buffer_payloads);
                self.resource_cache
                    .apply_render_product_payloads(render_product_payloads);

                // Prune removed nodes
                let handles_set: BTreeSet<NodeId> = node_handles.iter().copied().collect();
                self.nodes.retain(|handle, _| handles_set.contains(handle));

                // Apply changes
                for change in node_changes {
                    match change {
                        NodeChange::Created { handle, path, kind } => {
                            // Create new entry with placeholder config
                            let config: Box<dyn NodeConfig> = match kind {
                                NodeKind::Texture => Box::new(TextureConfig { width: 0, height: 0 }),
                                NodeKind::Shader => Box::new(ShaderConfig::default()),
                                NodeKind::Output => Box::new(OutputConfig::GpioStrip {
                                    pin: 0,
                                    options: None,
                                }),
                                NodeKind::Fixture => Box::new(FixtureConfig {
                                    output_spec: lpc_model::NodeSpec::from(""),
                                    texture_spec: lpc_model::NodeSpec::from(""),
                                    mapping: lpc_source::legacy::nodes::fixture::MappingConfig::PathPoints {
                                        paths: vec![],
                                        sample_diameter: 2.0,
                                    },
                                    color_order: lpc_source::legacy::nodes::fixture::ColorOrder::Rgb,
                                    transform: [[0.0; 4]; 4],
                                    brightness: None,
                                    gamma_correction: None,
                                }),
                            };

                            let initial_status = WireNodeStatus::Created;
                            self.nodes.insert(
                                *handle,
                                NodeEntryView {
                                    path: path.clone(),
                                    kind: *kind,
                                    config,
                                    config_ver: FrameId::default(),
                                    state: None,
                                    state_ver: FrameId::default(),
                                    status: initial_status.clone(),
                                    status_ver: FrameId::default(),
                                },
                            );
                            // Track initial status
                            self.previous_status.insert(*handle, initial_status);
                        }
                        NodeChange::ConfigUpdated { handle, config_ver } => {
                            if let Some(entry) = self.nodes.get_mut(handle) {
                                entry.config_ver = *config_ver;
                            }
                        }
                        NodeChange::StateUpdated { handle, state_ver } => {
                            if let Some(entry) = self.nodes.get_mut(handle) {
                                entry.state_ver = *state_ver;
                            }
                        }
                        NodeChange::StatusChanged { handle, status } => {
                            if let Some(entry) = self.nodes.get_mut(handle) {
                                let old_status = entry.status.clone();
                                let new_status = status.clone();

                                // Track all status changes - StatusChanged event indicates a change occurred
                                status_changes.push(StatusChangeView {
                                    path: entry.path.clone(),
                                    old_status: old_status.clone(),
                                    new_status: new_status.clone(),
                                });

                                entry.status = new_status.clone();
                                // Update status_ver - we use frame_id as proxy since StatusChanged
                                // doesn't include status_ver. The actual status_ver from server
                                // triggered this event, so we use current frame_id.
                                entry.status_ver = self.frame_id;
                                // Update previous status for next comparison
                                self.previous_status.insert(*handle, new_status);
                            } else {
                                // Node doesn't exist yet - just track the status
                                self.previous_status.insert(*handle, status.clone());
                            }
                        }
                        NodeChange::Removed { handle } => {
                            self.nodes.remove(&handle);
                            self.detail_tracking.remove(&handle);
                            self.previous_status.remove(&handle);
                        }
                    }
                }

                // Update details (create entries if they don't exist)
                for (handle, detail) in node_details {
                    if let Some(entry) = self.nodes.get_mut(handle) {
                        entry.config =
                            clone_node_config_for_kind(detail.config.as_ref(), entry.kind)?;
                        // Merge partial update into existing state
                        if let Some(existing_state) = &mut entry.state {
                            // Merge fields from partial update into existing state
                            existing_state.merge_from(&detail.state, *current_frame);
                        } else {
                            // No existing state, use the new state as-is
                            entry.state = Some(detail.state.clone());
                        }
                        // Status is no longer in node_details, it comes via StatusChanged events
                    } else {
                        // Create new entry from detail (node exists but wasn't in Created changes)
                        // Note: NodeDetail doesn't have kind field, so we infer from state
                        let kind = match &detail.state {
                            NodeState::Texture(_) => NodeKind::Texture,
                            NodeState::Shader(_) => NodeKind::Shader,
                            NodeState::Output(_) => NodeKind::Output,
                            NodeState::Fixture(_) => NodeKind::Fixture,
                        };

                        let config = clone_node_config_for_kind(detail.config.as_ref(), kind)?;

                        // `StatusChanged` may run before any entry exists (no `Created` in this batch
                        // when `config_ver != state_ver` on the server). Honor that pending status here.
                        let initial_status = self
                            .previous_status
                            .remove(handle)
                            .unwrap_or(WireNodeStatus::Created);

                        self.nodes.insert(
                            *handle,
                            NodeEntryView {
                                path: detail.path.clone(),
                                kind,
                                config,
                                config_ver: FrameId::default(),
                                state: Some(detail.state.clone()),
                                state_ver: FrameId::default(),
                                status: initial_status.clone(),
                                status_ver: *current_frame,
                            },
                        );
                        self.previous_status.insert(*handle, initial_status);
                    }
                }

                Ok(status_changes)
            }
        }
    }

    /// Get texture data for a node handle
    ///
    /// Returns the texture data bytes, or an error if:
    /// - The node doesn't exist
    /// - The node is not a texture node
    /// - The node doesn't have state (not being tracked for detail)
    pub fn get_texture_data(&self, handle: NodeId) -> Result<Vec<u8>, String> {
        let entry = self
            .nodes
            .get(&handle)
            .ok_or_else(|| format!("Node handle {} not found in client view", handle.as_u32()))?;

        if entry.kind != NodeKind::Texture {
            return Err(format!(
                "Node {} is not a texture node (kind: {:?})",
                entry.path.as_str(),
                entry.kind
            ));
        }

        match &entry.state {
            Some(NodeState::Texture(tex_state)) => resource_cache::resolve_legacy_compat_bytes(
                &tex_state.texture_data,
                &self.resource_cache,
            ),
            Some(_) => Err(format!(
                "Node {} has wrong state type (expected Texture)",
                entry.path.as_str()
            )),
            None => Err(format!(
                "Node {} does not have state (not being tracked for detail)",
                entry.path.as_str()
            )),
        }
    }

    /// Get output channel data for a node handle
    ///
    /// Returns the output channel data bytes, or an error if:
    /// - The node doesn't exist
    /// - The node is not an output node
    /// - The node doesn't have state (not being tracked for detail)
    pub fn get_output_data(&self, handle: NodeId) -> Result<Vec<u8>, String> {
        let entry = self
            .nodes
            .get(&handle)
            .ok_or_else(|| format!("Node handle {} not found in client view", handle.as_u32()))?;

        if entry.kind != NodeKind::Output {
            return Err(format!(
                "Node {} is not an output node (kind: {:?})",
                entry.path.as_str(),
                entry.kind
            ));
        }

        match &entry.state {
            Some(NodeState::Output(output_state)) => resource_cache::resolve_output_channel_bytes(
                &output_state.channel_data,
                &self.resource_cache,
            ),
            Some(_) => Err(format!(
                "Node {} has wrong state type (expected Output)",
                entry.path.as_str()
            )),
            None => Err(format!(
                "Node {} does not have state (not being tracked for detail)",
                entry.path.as_str()
            )),
        }
    }
}

fn clone_node_config_for_kind(
    config: &dyn NodeConfig,
    expected_kind: NodeKind,
) -> Result<Box<dyn NodeConfig>, String> {
    let actual_kind = config.kind();
    if actual_kind != expected_kind {
        return Err(format!(
            "node detail config kind mismatch: expected {expected_kind:?}, got {actual_kind:?}"
        ));
    }

    match expected_kind {
        NodeKind::Texture => {
            let config = config
                .as_any()
                .downcast_ref::<TextureConfig>()
                .ok_or_else(|| String::from("failed to downcast TextureConfig"))?;
            Ok(Box::new(config.clone()))
        }
        NodeKind::Shader => {
            let config = config
                .as_any()
                .downcast_ref::<ShaderConfig>()
                .ok_or_else(|| String::from("failed to downcast ShaderConfig"))?;
            Ok(Box::new(config.clone()))
        }
        NodeKind::Output => {
            let config = config
                .as_any()
                .downcast_ref::<OutputConfig>()
                .ok_or_else(|| String::from("failed to downcast OutputConfig"))?;
            Ok(Box::new(config.clone()))
        }
        NodeKind::Fixture => {
            let config = config
                .as_any()
                .downcast_ref::<FixtureConfig>()
                .ok_or_else(|| String::from("failed to downcast FixtureConfig"))?;
            Ok(Box::new(config.clone()))
        }
    }
}
