use super::types::{LegacyProjectRuntime, MemoryStatsFn, NodeEntry, NodeStatus};
use crate::error::Error;
use crate::output::OutputProvider;
use crate::runtime::frame_time::FrameTime;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::{vec, vec::Vec};
use core::cell::RefCell;
use lp_perf::EVENT_PROJECT_LOAD;
use lpc_model::{FrameId, LpPath, LpPathBuf, NodeId};
use lpc_source::legacy::nodes::{NodeConfig, NodeKind};
use lpc_source::legacy::nodes::{
    fixture::FixtureConfig, output::OutputConfig, shader::ShaderConfig, texture::TextureConfig,
};
use lpc_wire::WireNodeSpecifier;
use lpc_wire::legacy::ProjectResponse;
use lpfs::FsChange;

impl LegacyProjectRuntime {
    /// Create new project runtime
    pub fn new(
        fs: Rc<RefCell<dyn lpfs::LpFs>>,
        output_provider: Rc<RefCell<dyn OutputProvider>>,
        memory_stats: Option<MemoryStatsFn>,
        time_provider: Option<Rc<dyn lpc_shared::time::TimeProvider>>,
        graphics: Arc<dyn crate::gfx::LpGraphics>,
    ) -> Result<Self, Error> {
        lp_perf::emit_begin!(EVENT_PROJECT_LOAD);
        let result = (|| -> Result<Self, Error> {
            let _config =
                crate::legacy_project::legacy_loader::legacy_load_from_filesystem(&*fs.borrow())?;

            Ok(Self {
                frame_id: FrameId::default(),
                frame_time: FrameTime::zero(),
                fs,
                output_provider,
                nodes: BTreeMap::new(),
                next_handle: 1,
                memory_stats,
                time_provider,
                graphics,
            })
        })();
        lp_perf::emit_end!(EVENT_PROJECT_LOAD);
        result
    }

    /// Destroy all node runtimes, releasing resources (e.g. output channels).
    ///
    /// Call before dropping the project to ensure output provider resources are freed.
    pub fn destroy_all_nodes(&mut self) -> Result<(), Error> {
        let provider = self.output_provider.borrow();
        let output_provider: &dyn OutputProvider = &*provider;
        for (_, entry) in &mut self.nodes {
            if let Some(mut runtime) = entry.runtime.take() {
                runtime.destroy(Some(output_provider))?;
            }
        }
        Ok(())
    }

    /// Load nodes from filesystem (doesn't initialize them)
    pub fn load_nodes(&mut self) -> Result<(), Error> {
        let node_paths = crate::legacy_project::legacy_loader::discover_nodes(&*self.fs.borrow())?;

        for path in node_paths {
            match crate::legacy_project::legacy_loader::legacy_load_node(&*self.fs.borrow(), &path)
            {
                Ok((path, config)) => {
                    let handle = NodeId::new(self.next_handle);
                    self.next_handle += 1;

                    let kind = config.kind();
                    let entry = NodeEntry {
                        path,
                        kind,
                        config,
                        config_ver: self.frame_id,
                        status: NodeStatus::Created,
                        status_ver: self.frame_id,
                        runtime: None,
                        state_ver: FrameId::default(),
                    };

                    self.nodes.insert(handle, entry);
                }
                Err(e) => {
                    // Create entry with error status
                    let handle = NodeId::new(self.next_handle);
                    self.next_handle += 1;

                    // Try to determine kind from path
                    let kind =
                        match crate::legacy_project::legacy_loader::legacy_node_kind_from_path(
                            &path,
                        ) {
                            Ok(k) => k,
                            Err(_) => continue, // Skip unknown types
                        };

                    // Create a dummy config based on kind
                    // This is a temporary solution until we have a better way
                    let config: Box<dyn NodeConfig> = match kind {
                        NodeKind::Texture => Box::new(TextureConfig {
                            width: 0,
                            height: 0,
                        }),
                        NodeKind::Shader => Box::new(ShaderConfig::default()),
                        NodeKind::Output => Box::new(OutputConfig::GpioStrip {
                            pin: 0,
                            options: None,
                        }),
                        NodeKind::Fixture => Box::new(FixtureConfig {
                            output_spec: lpc_model::NodeSpec::from(""),
                            texture_spec: lpc_model::NodeSpec::from(""),
                            mapping:
                                lpc_source::legacy::nodes::fixture::MappingConfig::PathPoints {
                                    paths: vec![],
                                    sample_diameter: 2.0,
                                },
                            color_order: lpc_source::legacy::nodes::fixture::ColorOrder::Rgb,
                            transform: [[0.0; 4]; 4],
                            brightness: None,
                            gamma_correction: None,
                        }),
                    };

                    let entry = NodeEntry {
                        path,
                        kind,
                        config,
                        config_ver: self.frame_id,
                        status: NodeStatus::InitError(format!("Failed to load: {e}")),
                        status_ver: self.frame_id,
                        runtime: None,
                        state_ver: FrameId::default(),
                    };

                    self.nodes.insert(handle, entry);
                }
            }
        }

        Ok(())
    }

    /// Ensure all nodes initialized successfully
    ///
    /// Returns an error if any nodes failed to initialize, with details about
    /// which nodes failed and why. Warnings and runtime errors are ignored
    /// (nodes with warnings or runtime errors are considered successfully initialized).
    pub fn ensure_all_nodes_initialized(&self) -> Result<(), Error> {
        let mut failed_nodes = Vec::new();

        for (_, entry) in &self.nodes {
            match &entry.status {
                NodeStatus::Ok | NodeStatus::Warn(_) | NodeStatus::Error(_) => {
                    // Node initialized successfully
                    // Warnings and runtime errors (e.g., GLSL compilation errors) are acceptable
                    // The node is initialized, just in an error state
                }
                NodeStatus::Created => {
                    failed_nodes.push(format!(
                        "{} ({:?}): not initialized",
                        entry.path.as_str(),
                        entry.kind
                    ));
                }
                NodeStatus::InitError(msg) => {
                    failed_nodes.push(format!(
                        "{} ({:?}): initialization error: {}",
                        entry.path.as_str(),
                        entry.kind,
                        msg
                    ));
                }
            }
        }

        if failed_nodes.is_empty() {
            Ok(())
        } else {
            Err(Error::Other {
                message: format!(
                    "Some nodes failed to initialize:\n  {}",
                    failed_nodes.join("\n  ")
                ),
            })
        }
    }

    /// Resolve a path to a node handle
    ///
    /// Returns the handle for the node at the given path, or an error if not found.
    pub fn handle_for_path(&self, path: &LpPath) -> Result<NodeId, Error> {
        let node_path = LpPathBuf::from(path);

        // Look up node by path
        for (handle, entry) in &self.nodes {
            if entry.path == node_path {
                return Ok(*handle);
            }
        }

        Err(Error::NotFound {
            path: path.as_str().to_string(),
        })
    }

    /// Handle a delete change
    pub fn handle_delete_change(&mut self, change: &FsChange) -> Result<(), Error> {
        // Check if node.json was deleted
        if change.path.has_suffix("/node.json") {
            // Extract node path from file path
            if let Some(node_path) = self.extract_node_path_from_file_path(change.path.as_path()) {
                if let Ok(handle) = self.handle_for_path(node_path.as_path()) {
                    // Destroy runtime if it exists (close output channels etc.)
                    if let Some(entry) = self.nodes.get_mut(&handle) {
                        if let Some(mut runtime) = entry.runtime.take() {
                            let provider = self.output_provider.borrow();
                            runtime.destroy(Some(&*provider))?;
                        }
                    }
                    // Remove node
                    self.nodes.remove(&handle);
                }
            }
        } else if self.is_node_directory_path(change.path.as_path()) {
            // Node directory was deleted
            if let Some(node_path) = self.extract_node_path_from_file_path(change.path.as_path()) {
                if let Ok(handle) = self.handle_for_path(node_path.as_path()) {
                    // Destroy runtime if it exists (close output channels etc.)
                    if let Some(entry) = self.nodes.get_mut(&handle) {
                        if let Some(mut runtime) = entry.runtime.take() {
                            let provider = self.output_provider.borrow();
                            runtime.destroy(Some(&*provider))?;
                        }
                    }
                    // Remove node
                    self.nodes.remove(&handle);
                }
            }
        }

        Ok(())
    }

    /// Handle a create change
    pub fn handle_create_change(&mut self, change: &FsChange) -> Result<(), Error> {
        // Check if this is a new node directory
        if self.is_node_directory_path(change.path.as_path()) {
            // Check if node already exists
            if self.handle_for_path(change.path.as_path()).is_err() {
                // Load the new node
                self.load_node_by_path(change.path.as_path())?;
            }
        }

        Ok(())
    }

    /// Check if a file path belongs to a node directory
    pub fn file_belongs_to_node(&self, file_path: &LpPath, node_path: &LpPath) -> bool {
        file_path.starts_with(node_path)
    }

    /// Extract node path from a file path
    ///
    /// Given a file path like "/src/my-shader.shader/node.json" or "/src/my-shader.shader/main.glsl",
    /// returns the node path "/src/my-shader.shader".
    pub fn extract_node_path_from_file_path(&self, file_path: &LpPath) -> Option<LpPathBuf> {
        file_path.parent().map(|p| p.to_path_buf())
    }

    /// Check if a path is a node directory (ends with .shader, .texture, etc.)
    pub fn is_node_directory_path(&self, path: &LpPath) -> bool {
        path.has_suffix(".shader")
            || path.has_suffix(".texture")
            || path.has_suffix(".output")
            || path.has_suffix(".fixture")
    }

    /// Load a single node by path
    pub fn load_node_by_path(&mut self, path: &LpPath) -> Result<NodeId, Error> {
        match crate::legacy_project::legacy_loader::legacy_load_node(&*self.fs.borrow(), path) {
            Ok((path, config)) => {
                let handle = NodeId::new(self.next_handle);
                self.next_handle += 1;

                let kind = config.kind();
                let entry = NodeEntry {
                    path,
                    kind,
                    config,
                    config_ver: self.frame_id,
                    status: NodeStatus::Created,
                    status_ver: self.frame_id,
                    runtime: None,
                    state_ver: FrameId::default(),
                };

                self.nodes.insert(handle, entry);
                Ok(handle)
            }
            Err(e) => Err(e),
        }
    }

    /// Initialize all nodes in dependency order
    pub fn init_nodes(&mut self) -> Result<(), Error> {
        crate::legacy::project::init_nodes(self)
    }

    /// Advance to next frame and render
    pub fn tick(&mut self, delta_ms: u32) -> Result<(), Error> {
        crate::legacy::project::tick(self, delta_ms)
    }

    /// Handle filesystem changes
    pub fn handle_fs_changes(&mut self, changes: &[FsChange]) -> Result<(), Error> {
        crate::legacy::project::handle_fs_changes(self, changes)
    }

    /// Get changes since a frame (for client sync)
    pub fn get_changes(
        &self,
        since_frame: FrameId,
        detail_specifier: &WireNodeSpecifier,
        theoretical_fps: Option<f32>,
    ) -> Result<ProjectResponse, Error> {
        crate::legacy::project::get_changes(self, since_frame, detail_specifier, theoretical_fps)
    }
}
