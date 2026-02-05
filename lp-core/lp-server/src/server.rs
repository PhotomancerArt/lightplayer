//! Main server struct for processing messages and managing projects

extern crate alloc;

use crate::error::ServerError;
use crate::handlers;
use crate::project_manager::ProjectManager;
use alloc::{boxed::Box, format, rc::Rc, string::ToString, vec::Vec};
use core::cell::RefCell;
use hashbrown::HashMap;
use log;
use lp_model::{LpPath, LpPathBuf, Message};
use lp_shared::fs::{FsChange, LpFs};
use lp_shared::output::OutputProvider;

/// Main server struct for processing client-server messages
///
/// Uses a tick-based API similar to game engines, processing incoming messages
/// and returning responses. The server manages projects and handles filesystem
/// operations on its base filesystem.
pub struct LpServer {
    /// Output provider (shared, mutable) for projects
    output_provider: Rc<RefCell<dyn OutputProvider>>,
    /// Project manager for handling multiple projects
    project_manager: ProjectManager,
    /// Base filesystem (server root, projects in `projects/` subdirectory)
    base_fs: Box<dyn LpFs>,
    /// Last frame processing time in microseconds (for theoretical FPS calculation)
    last_frame_time_us: RefCell<Option<u64>>,
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
    /// use lp_model::AsLpPath;
    /// use lp_server::LpServer;
    /// use lp_shared::fs::LpFsStd;
    /// use lp_shared::output::MemoryOutputProvider;
    /// use alloc::{boxed::Box, rc::Rc};
    /// use core::cell::RefCell;
    ///
    /// let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));
    /// let base_fs = Box::new(LpFsStd::new("/path/to/server/root".into()));
    /// let server = LpServer::new(output_provider, base_fs, "projects/".as_path());
    /// ```
    pub fn new(
        output_provider: Rc<RefCell<dyn OutputProvider>>,
        base_fs: Box<dyn LpFs>,
        projects_base_dir: &LpPath,
    ) -> Self {
        let project_manager = ProjectManager::new(projects_base_dir);
        Self {
            output_provider,
            project_manager,
            base_fs,
            last_frame_time_us: RefCell::new(None),
        }
    }

    /// Process incoming messages and return responses
    ///
    /// This is the main entry point for processing client messages. It handles
    /// filesystem operations and project management requests.
    ///
    /// # Arguments
    ///
    /// * `delta_ms` - Time delta in milliseconds (for future use with project updates)
    /// * `incoming` - Vector of incoming messages from clients
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<Message>)` - Vector of response messages (all `Message::Server` variants)
    /// * `Err(ServerError)` - If processing failed
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// extern crate alloc;
    /// use lp_model::{AsLpPath, Message};
    /// use lp_server::LpServer;
    /// use lp_shared::fs::LpFsMemory;
    /// use lp_shared::output::MemoryOutputProvider;
    /// use alloc::{boxed::Box, rc::Rc, vec::Vec};
    /// use core::cell::RefCell;
    ///
    /// let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));
    /// let base_fs = Box::new(LpFsMemory::new());
    /// let mut server = LpServer::new(output_provider, base_fs, "projects/".as_path());
    /// let incoming = vec![/* messages */];
    /// let responses = server.tick(16, incoming).unwrap();
    /// ```
    pub fn tick(
        &mut self,
        delta_ms: u32,
        incoming: Vec<Message>,
    ) -> Result<Vec<Message>, ServerError> {
        // Process filesystem changes for all loaded projects
        // Collect project info first to avoid borrowing issues
        let project_info: Vec<_> = self
            .project_manager
            .list_loaded_projects()
            .iter()
            .map(|p| (p.handle, p.path.clone()))
            .collect();
        
        log::debug!("LpServer::tick: Found {} loaded projects", project_info.len());

        // Collect changes per project
        let mut project_changes_map: HashMap<_, Vec<FsChange>> = HashMap::new();

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
                let project_changes: Vec<FsChange> = base_changes
                    .into_iter()
                    .filter_map(|change| {
                        // Use LpPath to strip prefix and normalize
                        if let Some(stripped) = change.path.strip_prefix(project_prefix) {
                            Some(FsChange {
                                path: stripped.to_path_buf(),
                                change_type: change.change_type,
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

        // Get current_version AFTER collecting all changes but BEFORE processing
        // This represents the version that will be assigned to the NEXT change
        // So all changes we're about to process have versions < current_version
        let current_version = self.base_fs().current_version();

        // Now apply changes to projects (mutable borrows)
        for (handle, project_changes) in project_changes_map {
            if let Some(project) = self.project_manager.get_project_mut(handle) {
                if let Err(_e) = project.runtime_mut().handle_fs_changes(&project_changes) {
                    // Log error but continue with other projects
                    // Note: In no_std context, errors are silently ignored
                    // Errors will be visible when clients sync or query project state
                } else {
                    // Update last processed version to current_version.next() (one more than the next version)
                    // This ensures that get_changes_since(current_version.next()) will return nothing next time
                    // because get_changes_since uses >=, and current_version.next() is beyond all changes we processed
                    // All changes we processed have versions < current_version, so >= current_version.next() returns nothing
                    project.update_fs_version(current_version.next());
                }
            }
        }

        // Tick all loaded projects
        // Tick each project's runtime BEFORE processing incoming messages
        // This ensures GetChanges requests see the current frame's data
        log::debug!("LpServer::tick: Ticking {} projects", project_info.len());
        for (handle, path) in &project_info {
            if let Some(project) = self.project_manager.get_project_mut(*handle) {
                log::debug!(
                    "LpServer::tick: Ticking project {} (path: {}, delta_ms: {})",
                    project.name(),
                    path,
                    delta_ms
                );
                // Ignore errors and continue with other projects
                // Errors will be visible when clients sync or query project state
                match project.runtime_mut().tick(delta_ms) {
                    Ok(()) => {
                        log::trace!("LpServer::tick: Project {} tick succeeded", project.name());
                    }
                    Err(e) => {
                        log::warn!("LpServer::tick: Project {} tick error: {:?}", project.name(), e);
                    }
                }
            } else {
                log::warn!("LpServer::tick: Project handle {} not found", handle.as_i32());
            }
        }

        // Log frame IDs after ticking (for debugging frame synchronization)
        for (handle, _) in &project_info {
            if let Some(project) = self.project_manager.get_project(*handle) {
                log::debug!(
                    "LpServer::tick: Project {} frame_id: {}",
                    project.name(),
                    project.runtime().frame_id.as_i64()
                );
            }
        }

        // Process incoming messages AFTER ticking projects
        // This ensures GetChanges requests see the current frame's data
        let mut responses = Vec::new();
        for message in incoming {
            match message {
                Message::Client(client_msg) => {
                    // Process client message and generate response
                    let theoretical_fps = self.theoretical_fps();
                    let msg_id = client_msg.id;
                    match handlers::handle_client_message(
                        &mut self.project_manager,
                        &mut *self.base_fs,
                        &self.output_provider,
                        client_msg,
                        theoretical_fps,
                    ) {
                        Ok(response) => {
                            responses.push(Message::Server(response));
                        }
                        Err(e) => {
                            // Send error response for this message
                            responses.push(Message::Server(lp_model::ServerMessage {
                                id: msg_id,
                                msg: lp_model::server::ServerMsgBody::Error {
                                    error: format!("{e}"),
                                },
                            }));
                        }
                    }
                }
                Message::Server(_) => {
                    // Server messages shouldn't be sent to the server
                    // Log or ignore
                    return Err(ServerError::Core(
                        "Received server message on server side".to_string(),
                    ));
                }
            }
        }

        Ok(responses)
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
}
