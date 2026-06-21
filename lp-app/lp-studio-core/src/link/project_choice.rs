use lpc_wire::{LoadedProject, WireProjectHandle};
use serde::{Deserialize, Serialize};

/// A project the user or Studio can attach to on a running LightPlayer server.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ProjectChoice {
    pub project_id: String,
    pub server_path: String,
    pub handle: WireProjectHandle,
}

impl ProjectChoice {
    pub fn new(
        project_id: impl Into<String>,
        server_path: impl Into<String>,
        handle: WireProjectHandle,
    ) -> Self {
        Self {
            project_id: project_id.into(),
            server_path: server_path.into(),
            handle,
        }
    }

    pub fn from_loaded_project(project: LoadedProject) -> Self {
        let server_path = project.path.as_str().to_string();
        let project_id = project_id_from_path(&server_path, project.handle);
        Self::new(project_id, server_path, project.handle)
    }
}

fn project_id_from_path(path: &str, handle: WireProjectHandle) -> String {
    let trimmed = path.trim_matches('/');
    let project_id = trimmed.strip_prefix("projects/").unwrap_or(trimmed);
    if project_id.is_empty() {
        format!("project-{}", handle.id())
    } else {
        project_id.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_project_id_from_standard_server_path() {
        assert_eq!(
            project_id_from_path("/projects/studio-demo", WireProjectHandle::new(1)),
            "studio-demo"
        );
    }

    #[test]
    fn keeps_nonstandard_path_as_project_id() {
        assert_eq!(
            project_id_from_path("/scratch/live", WireProjectHandle::new(4)),
            "scratch/live"
        );
    }
}
