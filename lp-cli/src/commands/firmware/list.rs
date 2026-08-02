//! `lp-cli firmware list` — enumerate the checked-in build defs.

use anyhow::Result;

use super::args::ListArgs;
use super::build_def::{find_repo_root, load_build_defs};

pub fn handle_list(args: ListArgs) -> Result<()> {
    let repo_root = find_repo_root()?;
    let defs = load_build_defs(&repo_root)?;

    if args.json {
        let rows: Vec<serde_json::Value> = defs
            .iter()
            .map(|def| {
                serde_json::json!({
                    "id": def.id,
                    "displayName": def.display_name,
                    "package": def.package,
                    "chip": def.chip.name,
                    "cargoTarget": def.cargo_target,
                    "profile": def.profile,
                    "cargoFeatures": def.cargo_features,
                    "flashSizeMb": def.flash_size_mb,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }

    let id_width = defs.iter().map(|def| def.id.len()).max().unwrap_or(2);
    for def in &defs {
        println!(
            "{:id_width$}  {:8}  {:>3} MB  {}",
            def.id,
            def.chip.name,
            def.flash_size_mb,
            def.display_name,
            id_width = id_width
        );
    }
    Ok(())
}
