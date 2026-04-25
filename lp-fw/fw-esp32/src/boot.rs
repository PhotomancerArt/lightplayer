//! Boot-time configuration and auto-load logic.
//!
//! Reads lightplayer.json for startup_project, or falls back to lexical-first
//! project in projects/ directory.

use lp_model::{LpPathBuf, config::LightplayerConfig, path::AsLpPath};
use lp_server::LpServer;
use lpfs::LpFs;

/// Config file path at filesystem root
const CONFIG_PATH: &str = "/lightplayer.json";

/// Read LightplayerConfig from /lightplayer.json.
///
/// Returns None if file is missing, unreadable, or invalid JSON.
pub fn read_config(fs: &dyn LpFs) -> Option<LightplayerConfig> {
    let data = fs.read_file(CONFIG_PATH.as_path()).ok()?;
    lp_model::json::from_slice(&data).ok()
}

/// Auto-load a project at boot: use startup_project from config if set,
/// otherwise load the first project by lexical order in projects/.
pub fn auto_load_project(server: &mut LpServer) {
    let raw_base = server.project_manager().projects_base_dir();
    let base_dir = if raw_base.starts_with('/') {
        LpPathBuf::from(raw_base)
    } else {
        LpPathBuf::from(alloc::format!("/{raw_base}").as_str())
    };
    let base_path = base_dir.as_path();
    log::info!("Boot: scanning {} for projects", base_path.as_str());

    let project_path = if let Some(config) = read_config(server.base_fs()) {
        if let Some(ref name) = config.startup_project {
            let path = base_dir.join(name);
            if server
                .base_fs()
                .file_exists(path.as_path())
                .unwrap_or(false)
                || server.base_fs().is_dir(path.as_path()).unwrap_or(false)
            {
                log::info!("Boot: found configured startup project: {name}");
                Some(path)
            } else {
                log::info!("Boot: configured startup project '{name}' not found");
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let project_path = match project_path {
        Some(p) => p,
        None => {
            let entries = match server.base_fs().list_dir(base_path, false) {
                Ok(e) => e,
                Err(e) => {
                    log::warn!("Boot: failed to list {}: {e}", base_path.as_str());
                    return;
                }
            };
            log::info!(
                "Boot: found {} entries in {}",
                entries.len(),
                base_path.as_str()
            );
            let mut projects: alloc::vec::Vec<_> = entries
                .into_iter()
                .filter(|e| {
                    let json_path = e.join("project.json");
                    server
                        .base_fs()
                        .file_exists(json_path.as_path())
                        .unwrap_or(false)
                })
                .collect();
            projects.sort_by(|a, b| a.as_str().cmp(b.as_str()));
            log::info!("Boot: {} valid projects found", projects.len());
            match projects.into_iter().next() {
                Some(p) => p,
                None => {
                    log::info!("Boot: no projects to auto-load");
                    return;
                }
            }
        }
    };

    log::info!("Boot: auto-loading {}", project_path.as_str());
    if let Err(e) = server.load_project(project_path.as_path()) {
        log::warn!("Boot: auto-load failed for {}: {e}", project_path.as_str());
    } else {
        log::info!("Boot: auto-loaded project {}", project_path.as_str());
    }
}
