//! Project wrapper for managing a single project instance

extern crate alloc;

use crate::error::ServerError;
use alloc::{format, rc::Rc, string::String};
use core::cell::RefCell;
use lp_engine::ProjectRuntime;
use lp_model::{LpPath, LpPathBuf};
use lp_shared::fs::{FsVersion, LpFs};
use lp_shared::output::OutputProvider;

/// A project instance wrapping a ProjectRuntime
pub struct Project {
    /// Project name/identifier
    name: String,
    /// Project filesystem path
    path: LpPathBuf,
    /// The underlying ProjectRuntime instance
    runtime: ProjectRuntime,
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
    ) -> Result<Self, ServerError> {
        let runtime = ProjectRuntime::new(fs, output_provider)
            .map_err(|e| ServerError::Core(format!("{e}")))?;

        Ok(Self {
            name,
            path: path.to_path_buf(),
            runtime,
            last_fs_version: FsVersion::default(),
        })
    }

    /// Get the project name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the project path
    pub fn path(&self) -> &LpPath {
        &self.path
    }

    /// Get mutable access to the underlying ProjectRuntime
    pub fn runtime_mut(&mut self) -> &mut ProjectRuntime {
        &mut self.runtime
    }

    /// Get immutable access to the underlying ProjectRuntime
    pub fn runtime(&self) -> &ProjectRuntime {
        &self.runtime
    }

    /// Reload the project from the filesystem
    pub fn reload(&mut self) -> Result<(), ServerError> {
        // todo!("Implement project reload")
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
