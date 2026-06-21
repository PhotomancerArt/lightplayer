use lpc_wire::{LoadedProject, WireProjectHandle};
use serde::{Deserialize, Serialize};

use crate::{ProjectChoice, RecoveryReason};

/// Result of inspecting project state on an already-connected server.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum ProjectStateResult {
    LoadedProject { project: ProjectChoice },
    NoLoadedProject,
    MultipleProjects { projects: Vec<ProjectChoice> },
    RecoveryRequired { reason: RecoveryReason },
}

impl ProjectStateResult {
    pub fn loaded_project(
        project_id: impl Into<String>,
        server_path: impl Into<String>,
        handle: WireProjectHandle,
    ) -> Self {
        Self::LoadedProject {
            project: ProjectChoice::new(project_id, server_path, handle),
        }
    }

    pub fn from_loaded_projects(projects: Vec<LoadedProject>) -> Self {
        match projects.len() {
            0 => Self::NoLoadedProject,
            1 => {
                let project = projects
                    .into_iter()
                    .next()
                    .expect("one loaded project after len check");
                Self::LoadedProject {
                    project: ProjectChoice::from_loaded_project(project),
                }
            }
            _ => Self::MultipleProjects {
                projects: projects
                    .into_iter()
                    .map(ProjectChoice::from_loaded_project)
                    .collect(),
            },
        }
    }
}
