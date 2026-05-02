//! Project wrapper for managing a single project instance

extern crate alloc;

use crate::error::ServerError;
use alloc::{boxed::Box, format, rc::Rc, string::String, sync::Arc};
use core::cell::RefCell;
use lpc_engine::{
    CoreProjectLoader, CoreProjectRuntime, LpGraphics, MemoryStatsFn, RuntimeServices,
};
use lpc_model::{LpPath, LpPathBuf, TreePath};
use lpc_shared::output::{OutputChannelHandle, OutputDriverOptions, OutputFormat, OutputProvider};
use lpc_shared::time::TimeProvider;
use lpfs::{FsVersion, LpFs};

/// A project instance wrapping a ProjectRuntime
pub struct Project {
    /// Project name/identifier
    name: String,
    /// Project filesystem path
    path: LpPathBuf,
    /// The underlying ProjectRuntime instance
    runtime: CoreProjectRuntime,
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
        graphics: Arc<dyn LpGraphics>,
    ) -> Result<Self, ServerError> {
        let _ = memory_stats;
        let _ = time_provider;

        let root_path = project_root_path(&name)?;
        let mut services = RuntimeServices::new(root_path);
        services.set_output_provider(Some(Box::new(SharedOutputProvider(output_provider))));

        let mut runtime = {
            let fs_ref = fs.borrow();
            CoreProjectLoader::load_from_root(&*fs_ref, services)
                .map_err(|e| ServerError::Core(format!("Failed to load core project: {e}")))?
        };
        runtime.engine_mut().set_graphics(Some(graphics));

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
    pub fn runtime_mut(&mut self) -> &mut CoreProjectRuntime {
        &mut self.runtime
    }

    /// Get immutable access to the underlying ProjectRuntime
    pub fn runtime(&self) -> &CoreProjectRuntime {
        &self.runtime
    }

    /// Reload the project from the filesystem.
    ///
    /// M4 accepts source changes but does not rebuild the core runtime yet.
    pub fn reload(&mut self) -> Result<(), ServerError> {
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

struct SharedOutputProvider(Rc<RefCell<dyn OutputProvider>>);

impl OutputProvider for SharedOutputProvider {
    fn open(
        &self,
        pin: u32,
        byte_count: u32,
        format: OutputFormat,
        options: Option<OutputDriverOptions>,
    ) -> Result<OutputChannelHandle, lpc_shared::error::OutputError> {
        self.0.borrow().open(pin, byte_count, format, options)
    }

    fn write(
        &self,
        handle: OutputChannelHandle,
        data: &[u16],
    ) -> Result<(), lpc_shared::error::OutputError> {
        self.0.borrow().write(handle, data)
    }

    fn close(&self, handle: OutputChannelHandle) -> Result<(), lpc_shared::error::OutputError> {
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
