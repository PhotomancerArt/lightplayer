use anyhow::{Result, anyhow, bail};
use dialoguer::{Confirm, Input, Select};
use lpc_hardware::{HardwareManifestFile, HardwareTarget};
use std::io::IsTerminal;

use crate::commands::hardware::args::{
    DeleteManifestArgs, HardwareTargetArg, ManifestArgs, ManifestSubcommand, NewManifestArgs,
    SetManifestArgs,
};

use super::board_manifest_store::{BoardManifestStore, slugify};

pub fn handle_manifest(args: ManifestArgs) -> Result<()> {
    let store = BoardManifestStore::discover(args.repo, args.boards_dir)?;
    match args.command {
        Some(ManifestSubcommand::List) => list_manifests(&store),
        Some(ManifestSubcommand::Show { id }) => show_manifest(&store, &id),
        Some(ManifestSubcommand::Validate { id }) => validate_manifests(&store, id.as_deref()),
        Some(ManifestSubcommand::New(args)) => new_manifest(&store, args),
        Some(ManifestSubcommand::Set(args)) => set_manifest(&store, args),
        Some(ManifestSubcommand::Delete(args)) => delete_manifest(&store, args),
        None => interactive_manifest(&store),
    }
}

fn interactive_manifest(store: &BoardManifestStore) -> Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        println!(
            "Non-interactive terminal detected; listing manifests from {}.",
            store.boards_dir().display()
        );
        return list_manifests(store);
    }

    loop {
        let manifests = store.list()?;
        let mut items: Vec<String> = manifests
            .iter()
            .map(|manifest| format!("{} - {} {}", manifest.id, manifest.vendor, manifest.product))
            .collect();
        items.push("Add new manifest".into());
        items.push("Validate all manifests".into());
        items.push("Quit".into());

        let choice = Select::new()
            .with_prompt("Hardware manifests")
            .items(&items)
            .default(0)
            .interact()?;
        if choice < manifests.len() {
            interactive_manifest_actions(store, &manifests[choice].id)?;
        } else if choice == manifests.len() {
            interactive_new_manifest(store)?;
        } else if choice == manifests.len() + 1 {
            validate_manifests(store, None)?;
        } else {
            return Ok(());
        }
    }
}

fn interactive_manifest_actions(store: &BoardManifestStore, id: &str) -> Result<()> {
    loop {
        let actions = ["Show", "Edit metadata", "Validate", "Delete", "Back"];
        let choice = Select::new()
            .with_prompt(format!("Manifest {id}"))
            .items(&actions)
            .default(0)
            .interact()?;
        match choice {
            0 => show_manifest(store, id)?,
            1 => interactive_edit_metadata(store, id)?,
            2 => validate_manifests(store, Some(id))?,
            3 => {
                delete_manifest(
                    store,
                    DeleteManifestArgs {
                        id: id.into(),
                        yes: false,
                    },
                )?;
                return Ok(());
            }
            _ => return Ok(()),
        }
    }
}

fn interactive_new_manifest(store: &BoardManifestStore) -> Result<()> {
    let target_items = [
        HardwareTargetArg::Esp32c6.label(),
        HardwareTargetArg::Rv32imacEmu.label(),
    ];
    let target_choice = Select::new()
        .with_prompt("Target")
        .items(&target_items)
        .default(0)
        .interact()?;
    let target = if target_choice == 0 {
        HardwareTargetArg::Esp32c6
    } else {
        HardwareTargetArg::Rv32imacEmu
    };
    let vendor: String = Input::new().with_prompt("Vendor").interact_text()?;
    let product: String = Input::new().with_prompt("Product").interact_text()?;
    let url: String = Input::new()
        .with_prompt("URL")
        .allow_empty(true)
        .interact_text()?;
    let description: String = Input::new()
        .with_prompt("Description")
        .allow_empty(true)
        .interact_text()?;
    new_manifest(
        store,
        NewManifestArgs {
            target,
            vendor,
            product,
            url: non_empty(url),
            description: non_empty(description),
            id: None,
            force: false,
        },
    )
}

fn interactive_edit_metadata(store: &BoardManifestStore, id: &str) -> Result<()> {
    let manifest = store.load(id)?;
    let vendor: String = Input::new()
        .with_prompt("Vendor")
        .default(manifest.vendor.clone())
        .interact_text()?;
    let product: String = Input::new()
        .with_prompt("Product")
        .default(manifest.product.clone())
        .interact_text()?;
    let url: String = Input::new()
        .with_prompt("URL")
        .default(manifest.url.clone().unwrap_or_default())
        .allow_empty(true)
        .interact_text()?;
    let description: String = Input::new()
        .with_prompt("Description")
        .default(manifest.description.clone().unwrap_or_default())
        .allow_empty(true)
        .interact_text()?;
    set_manifest(
        store,
        SetManifestArgs {
            id: id.into(),
            target: None,
            vendor: Some(vendor),
            product: Some(product),
            url: non_empty(url),
            description: non_empty(description),
        },
    )
}

fn list_manifests(store: &BoardManifestStore) -> Result<()> {
    let manifests = store.list()?;
    if manifests.is_empty() {
        println!(
            "No hardware manifests found in {}",
            store.boards_dir().display()
        );
        return Ok(());
    }
    for manifest in manifests {
        println!(
            "{}\t{}\t{} {}\t{}",
            manifest.id,
            manifest.target,
            manifest.vendor,
            manifest.product,
            manifest.path.display()
        );
    }
    Ok(())
}

fn show_manifest(store: &BoardManifestStore, id: &str) -> Result<()> {
    let manifest = store.load(id)?;
    println!("id: {}", manifest.id);
    println!("target: {}", manifest.target);
    println!("vendor: {}", manifest.vendor);
    println!("product: {}", manifest.product);
    if let Some(description) = &manifest.description {
        println!("description: {description}");
    }
    if let Some(url) = &manifest.url {
        println!("url: {url}");
    }
    println!("gpio resources: {}", manifest.gpio.len());
    println!("other resources: {}", manifest.resource.len());
    Ok(())
}

fn validate_manifests(store: &BoardManifestStore, id: Option<&str>) -> Result<()> {
    if let Some(id) = id {
        store.load(id)?;
        println!("{id}: ok");
        return Ok(());
    }

    let results = store.validate_all()?;
    let mut failed = false;
    for (id, result) in results {
        match result {
            Ok(()) => println!("{id}: ok"),
            Err(error) => {
                failed = true;
                println!("{id}: {error}");
            }
        }
    }
    if failed {
        bail!("one or more hardware manifests are invalid");
    }
    Ok(())
}

fn new_manifest(store: &BoardManifestStore, args: NewManifestArgs) -> Result<()> {
    let id = args
        .id
        .unwrap_or_else(|| format!("{}/{}", slugify(&args.vendor), slugify(&args.product)));
    let mut manifest =
        HardwareManifestFile::new(id.clone(), args.target.into(), args.vendor, args.product);
    manifest.url = args.url;
    manifest.description = args
        .description
        .or_else(|| Some(format!("{} board profile.", manifest.product)));
    let path = store.save(&manifest, args.force)?;
    println!("created {id} at {}", path.display());
    Ok(())
}

fn set_manifest(store: &BoardManifestStore, args: SetManifestArgs) -> Result<()> {
    let mut manifest = store.load(&args.id)?;
    if let Some(target) = args.target {
        manifest.target = target.into();
    }
    if let Some(vendor) = args.vendor {
        manifest.vendor = vendor;
    }
    if let Some(product) = args.product {
        manifest.product = product;
    }
    if args.url.is_some() {
        manifest.url = args.url;
    }
    if args.description.is_some() {
        manifest.description = args.description;
    }
    let path = store.save(&manifest, true)?;
    println!("updated {} at {}", manifest.id, path.display());
    Ok(())
}

fn delete_manifest(store: &BoardManifestStore, args: DeleteManifestArgs) -> Result<()> {
    if !args.yes {
        let confirmed = Confirm::new()
            .with_prompt(format!("Delete manifest {}?", args.id))
            .default(false)
            .interact()?;
        if !confirmed {
            println!("delete cancelled");
            return Ok(());
        }
    }
    let path = store.delete(&args.id)?;
    println!("deleted {} at {}", args.id, path.display());
    Ok(())
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

#[allow(
    dead_code,
    reason = "keeps target vocabulary visible near command behavior"
)]
fn target_matches_calibration(target: HardwareTarget, calibration_target: &str) -> Result<()> {
    let expected = match calibration_target {
        "esp32c6" => HardwareTarget::Esp32c6,
        "rv32imac_emu" => HardwareTarget::Rv32imacEmu,
        other => return Err(anyhow!("unsupported calibration target: {other}")),
    };
    if target != expected {
        bail!("manifest target {target} does not match calibration target {expected}");
    }
    Ok(())
}
