//! Project manager for handling multiple projects

extern crate alloc;

use crate::error::ServerError;
use crate::project::Project;
use alloc::{
    boxed::Box,
    format,
    rc::Rc,
    string::{String, ToString},
    vec::Vec,
};
use core::cell::RefCell;
use hashbrown::HashMap;
use lp_model::project::ProjectHandle;
use lp_model::{LpPath, LpPathBuf};
use lp_shared::fs::LpFs;
use lp_shared::output::OutputProvider;

/// Manages multiple project instances
pub struct ProjectManager {
    /// Map of project handle -> Project instance
    projects: HashMap<ProjectHandle, Project>,
    /// Map of project name -> handle (for reverse lookup)
    name_to_handle: HashMap<String, ProjectHandle>,
    /// Next handle ID to assign (starts at 1)
    next_handle_id: u32,
    /// Base directory where projects are stored (relative path)
    projects_base_dir: LpPathBuf,
}

impl ProjectManager {
    /// Create a new project manager
    ///
    /// `projects_base_dir` is the base directory where all projects are stored.
    /// Each project will have its own subdirectory.
    pub fn new(projects_base_dir: &LpPath) -> Self {
        Self {
            projects: HashMap::new(),
            name_to_handle: HashMap::new(),
            next_handle_id: 1,
            projects_base_dir: projects_base_dir.to_path_buf(),
        }
    }

    /// Create a new project
    ///
    /// Creates the project directory structure using the provided filesystem.
    /// todo!("Refactor to use new ProjectRuntime API")
    pub fn create_project(&mut self, _name: String, _fs: Box<dyn LpFs>) -> Result<(), ServerError> {
        todo!("Refactor to use new ProjectRuntime API")
    }

    /// Load a project from the filesystem
    ///
    /// Creates a Project instance and loads it into memory.
    /// Takes a base filesystem and OutputProvider as Rc<RefCell> (from LpServer).
    /// Extracts project name from path (last component, strips projects_base_dir prefix if present).
    /// Returns the ProjectHandle for the loaded project.
    pub fn load_project(
        &mut self,
        path: &LpPath,
        base_fs: &mut dyn LpFs,
        output_provider: Rc<RefCell<dyn OutputProvider>>,
    ) -> Result<ProjectHandle, ServerError> {
        // Extract project name from path
        let name = self.extract_project_name_from_path(path.as_str())?;

        // Check if already loaded
        if let Some(handle) = self.name_to_handle.get(&name) {
            return Ok(*handle); // Already loaded, return existing handle
        }

        // Generate new handle
        let handle = ProjectHandle::new(self.next_handle_id);
        self.next_handle_id = self.next_handle_id.wrapping_add(1);

        // Build project path relative to projects_base_dir using join
        // Ensure result is absolute (chroot requires absolute paths)
        let project_path = if self.projects_base_dir.is_absolute() {
            self.projects_base_dir.join(&name)
        } else {
            // If base_dir is relative, make it absolute first
            LpPathBuf::from("/")
                .join(self.projects_base_dir.as_str())
                .join(&name)
        };

        // Create project-scoped filesystem using chroot
        let project_fs = base_fs
            .chroot(project_path.as_path())
            .map_err(|e| ServerError::Filesystem(format!("Failed to chroot to project: {e}")))?;

        // Create a new project instance
        let mut project = Project::new(
            name.clone(),
            project_path.as_path(),
            project_fs,
            output_provider,
        )?;

        // Auto-initialize the project runtime
        project.runtime_mut().load_nodes().map_err(|e| {
            ServerError::Core(format!("Failed to load nodes for project {name}: {e}"))
        })?;
        project.runtime_mut().init_nodes().map_err(|e| {
            ServerError::Core(format!(
                "Failed to initialize nodes for project {name}: {e}"
            ))
        })?;
        project
            .runtime_mut()
            .ensure_all_nodes_initialized()
            .map_err(|e| {
                ServerError::Core(format!(
                    "Failed to ensure all nodes initialized for project {name}: {e}"
                ))
            })?;

        // Store mappings
        self.projects.insert(handle, project);
        log::info!("Project loaded: {name}");
        self.name_to_handle.insert(name, handle);

        Ok(handle)
    }

    /// Extract project name from path
    ///
    /// Strips projects_base_dir prefix if present, then extracts the last component.
    fn extract_project_name_from_path(&self, path: &str) -> Result<String, ServerError> {
        let mut normalized_path = path.trim_end_matches('/').to_string();

        // Strip projects_base_dir prefix if present
        let base_dir_str = self.projects_base_dir.as_str();
        if normalized_path.starts_with(base_dir_str) {
            normalized_path = normalized_path[base_dir_str.len()..].to_string();
            normalized_path = normalized_path.trim_start_matches('/').to_string();
        }

        // Extract last component using LpPath::file_name()
        let project_path = lp_model::LpPathBuf::from(normalized_path);
        let name = project_path.file_name().ok_or_else(|| {
            ServerError::Core(format!(
                "Invalid project path: cannot extract name from '{path}'"
            ))
        })?;

        Ok(name.to_string())
    }

    /// Unload a project
    ///
    /// Removes the project from memory but doesn't delete it from the filesystem.
    /// Output channels and other resources are freed before removal.
    pub fn unload_project(&mut self, handle: ProjectHandle) -> Result<(), ServerError> {
        let mut project = self
            .projects
            .remove(&handle)
            .ok_or_else(|| ServerError::ProjectNotFound(format!("handle {}", handle.id())))?;

        project
            .runtime_mut()
            .destroy_all_nodes()
            .map_err(|e| ServerError::Core(format!("Failed to destroy project nodes: {e}")))?;

        let name = project.name();
        self.name_to_handle.remove(name);

        Ok(())
    }

    /// Unload all loaded projects
    ///
    /// Removes all projects from memory but doesn't delete them from the filesystem.
    /// Output channels and other resources are freed before removal.
    /// Note: next_handle_id is not reset - handles continue incrementing.
    pub fn unload_all_projects(&mut self) -> Result<(), ServerError> {
        for (_, mut project) in self.projects.drain() {
            project
                .runtime_mut()
                .destroy_all_nodes()
                .map_err(|e| ServerError::Core(format!("Failed to destroy project nodes: {e}")))?;
        }
        self.name_to_handle.clear();
        Ok(())
    }

    /// Get a project by handle
    pub fn get_project(&self, handle: ProjectHandle) -> Option<&Project> {
        self.projects.get(&handle)
    }

    /// Get a mutable reference to a project by handle
    pub fn get_project_mut(&mut self, handle: ProjectHandle) -> Option<&mut Project> {
        self.projects.get_mut(&handle)
    }

    /// Get handle by project name
    pub fn get_handle_by_name(&self, name: &str) -> Option<ProjectHandle> {
        self.name_to_handle.get(name).copied()
    }

    /// Get the projects base directory
    pub fn projects_base_dir(&self) -> &str {
        self.projects_base_dir.as_str()
    }

    /// List all loaded projects
    ///
    /// Returns a list of loaded projects with their handles and paths.
    pub fn list_loaded_projects(&self) -> Vec<lp_model::server::LoadedProject> {
        self.projects
            .iter()
            .map(|(handle, project)| lp_model::server::LoadedProject {
                handle: *handle,
                path: project.path().to_path_buf(),
            })
            .collect()
    }

    /// List all available projects on the filesystem
    ///
    /// Returns project names that exist on disk but may not be loaded.
    /// Requires a filesystem to query.
    pub fn list_available_projects(&self, fs: &dyn LpFs) -> Result<Vec<String>, ServerError> {
        // List entries in the base directory
        let entries = fs
            .list_dir(self.projects_base_dir.as_path(), false)
            .map_err(|e| {
                ServerError::Filesystem(format!("Failed to read projects directory: {e}"))
            })?;

        let mut projects = Vec::new();
        for entry in entries {
            // Check if this entry is a project directory (has project.json)
            let project_json_path = entry.join("project.json");
            if fs.file_exists(project_json_path.as_path()).unwrap_or(false) {
                // Extract project name from path
                // Entry format: "/base/project-name" or "/base/project-name/"
                let entry_str = entry.as_str();
                let name = entry_str
                    .trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .unwrap_or(entry_str)
                    .to_string();
                if !name.is_empty() {
                    projects.push(name);
                }
            }
        }

        Ok(projects)
    }
}
