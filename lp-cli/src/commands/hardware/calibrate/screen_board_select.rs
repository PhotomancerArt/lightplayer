use anyhow::{Result, bail};
use dialoguer::Select;
use lpc_hardware::HardwareTarget;
use std::io::{IsTerminal, stdin};

use crate::commands::hardware::manifest::board_manifest_store::BoardManifestStore;

pub fn choose_board(store: &BoardManifestStore, target: HardwareTarget) -> Result<String> {
    if !stdin().is_terminal() {
        bail!("--board is required when calibration is not running in an interactive terminal");
    }
    let manifests: Vec<_> = store
        .list()?
        .into_iter()
        .filter(|manifest| manifest.target == target.to_string())
        .collect();
    if manifests.is_empty() {
        bail!(
            "no {target} board manifests found in {}",
            store.boards_dir().display()
        );
    }
    let items: Vec<_> = manifests
        .iter()
        .map(|manifest| format!("{} - {} {}", manifest.id, manifest.vendor, manifest.product))
        .collect();
    let choice = Select::new()
        .with_prompt(format!("Board manifest for {target} calibration"))
        .items(&items)
        .default(0)
        .interact()?;
    Ok(manifests[choice].id.clone())
}
