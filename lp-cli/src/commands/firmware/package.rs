//! `lp-cli firmware package <id>` — build a firmware variant, merge it into a
//! flashable image, and emit the schemaVersion 2 distribution manifest.
//!
//! The manifest's `core` block is **extracted** from the artifact, never
//! restated: this command is the replacement for
//! `studio-firmware-manifest.mjs`'s hand-written feature list and the
//! `WIRE_PROTO_VERSION` `sed`.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use lpc_model::ManifestCore;
use lpc_model::manifest::find_manifest_core;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::args::PackageArgs;
use super::build::build_firmware;
use super::build_def::{BuildDef, find_repo_root, load_build_def};
use super::distribution_manifest::{
    DistributionManifest, FlashPolicy, MANIFEST_SCHEMA_VERSION, ManifestImage,
};

/// Where packaged firmware lands by default, relative to the repo root. The
/// Studio web build and the Pages artifact copy `firmware/<id>/` from here.
const DEFAULT_OUT_ROOT: &str = "target/studio-web-assets/firmware";

pub fn handle_package(args: PackageArgs) -> Result<()> {
    let repo_root = find_repo_root()?;
    let def = load_build_def(&repo_root, &args.id)?;

    if !args.no_build {
        build_firmware(&repo_root, &def)?;
    }

    let out_dir = match &args.out {
        Some(dir) => dir.clone(),
        None => repo_root.join(DEFAULT_OUT_ROOT).join(&def.id),
    };
    let manifest_path = package_build(&repo_root, &def, &out_dir)?;
    println!("firmware manifest: {}", manifest_path.display());
    Ok(())
}

/// Merge, extract, verify and write. Returns the manifest path.
fn package_build(repo_root: &Path, def: &BuildDef, out_dir: &Path) -> Result<PathBuf> {
    let elf = def.elf_path(repo_root);
    if !elf.exists() {
        bail!(
            "{} does not exist — run `lp-cli firmware build {}` first",
            elf.display(),
            def.id
        );
    }

    // The core the *build* says it is. Parsed as `ManifestCore` to prove the
    // shape, kept as raw JSON so the manifest carries it verbatim.
    let elf_bytes = std::fs::read(&elf).with_context(|| format!("reading {}", elf.display()))?;
    let (core_json, core) = extract_core(&elf_bytes, &elf)?;
    check_core_matches_def(def, &core, &elf)?;

    std::fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;
    remove_stale_outputs(out_dir)?;

    let image_name = format!("{}-merged.bin", def.package);
    let image_path = out_dir.join(&image_name);
    save_merged_image(repo_root, def, &elf, &image_path)?;

    // Package-time drift assertion: what espflash wrote must describe the
    // same build as the ELF we extracted from. A mismatch means the merge
    // picked up a stale or foreign artifact.
    let image_bytes =
        std::fs::read(&image_path).with_context(|| format!("reading {}", image_path.display()))?;
    let (_, image_core) = extract_core(&image_bytes, &image_path)?;
    if image_core != core {
        bail!(
            "manifest core drift: the merged image {} does not carry the same \
             manifest core as {}",
            image_path.display(),
            elf.display()
        );
    }

    let manifest = DistributionManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        firmware_id: def.id.clone(),
        display_name: def.display_name.clone(),
        generated_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        core: core_json,
        flash: FlashPolicy::merged_image(def.flash_size_bytes()),
        images: vec![ManifestImage {
            path: image_name,
            address: "0x0".to_string(),
            size_bytes: image_bytes.len() as u64,
            sha256: sha256_hex(&image_bytes),
        }],
    };

    let manifest_path = out_dir.join("manifest.json");
    let json = serde_json::to_string_pretty(&manifest)?;
    std::fs::write(&manifest_path, format!("{json}\n"))
        .with_context(|| format!("writing {}", manifest_path.display()))?;

    println!(
        "firmware image: {} ({} bytes, sha256={})",
        image_path.display(),
        manifest.images[0].size_bytes,
        manifest.images[0].sha256
    );
    Ok(manifest_path)
}

/// Extract and validate the embedded manifest core, returning it both as raw
/// JSON (for verbatim carriage) and parsed (for comparison).
fn extract_core(bytes: &[u8], source: &Path) -> Result<(Value, ManifestCore)> {
    let Some(payload) = find_manifest_core(bytes) else {
        bail!(
            "no firmware manifest core found in {} — the build predates the \
             embedded manifest or is not a LightPlayer firmware artifact",
            source.display()
        );
    };
    let text = core::str::from_utf8(payload)
        .with_context(|| format!("manifest payload in {} is not UTF-8", source.display()))?;
    let core: ManifestCore = serde_json::from_str(text).with_context(|| {
        format!(
            "manifest payload in {} is not a ManifestCore",
            source.display()
        )
    })?;
    let raw: Value = serde_json::from_str(text)?;
    Ok((raw, core))
}

/// The artifact must be the one this def describes. Cheap guard against
/// packaging a stale ELF from a different variant that happens to share the
/// target directory — the def's build inputs and the build's own account of
/// itself have to agree on identity (they never agree on *contents*; that is
/// the artifact's business alone).
fn check_core_matches_def(def: &BuildDef, core: &ManifestCore, elf: &Path) -> Result<()> {
    let mismatches: Vec<String> = [
        ("package", def.package.as_str(), core.package.as_str()),
        ("profile", def.profile.as_str(), core.profile.as_str()),
        (
            "cargoTarget",
            def.cargo_target.as_str(),
            core.target.cargo_target.as_str(),
        ),
        (
            "chip.family",
            def.chip.family.as_str(),
            core.target.family.as_str(),
        ),
        (
            "chip.name",
            def.chip.name.as_str(),
            core.target.chip.as_str(),
        ),
    ]
    .into_iter()
    .filter(|(_, expected, actual)| expected != actual)
    .map(|(field, expected, actual)| format!("{field}: def `{expected}` vs artifact `{actual}`"))
    .collect();

    if !mismatches.is_empty() {
        bail!(
            "{} was not built from build def `{}` — {}",
            elf.display(),
            def.id,
            mismatches.join("; ")
        );
    }
    Ok(())
}

/// `espflash save-image --merge --skip-padding` with the def's chip, flash
/// size and partition table. `--flash-size` is load-bearing: espflash writes
/// it into the image header and the bootloader validates the partition table
/// against that header, not the physical chip.
fn save_merged_image(
    repo_root: &Path,
    def: &BuildDef,
    elf: &Path,
    image_path: &Path,
) -> Result<()> {
    let partitions = repo_root.join(&def.partitions_csv);
    if !partitions.exists() {
        bail!("partition table {} does not exist", partitions.display());
    }
    let status = Command::new("espflash")
        .current_dir(repo_root)
        .arg("save-image")
        .args(["--chip", &def.chip.name])
        .arg("--partition-table")
        .arg(&partitions)
        .args(["--flash-size", &def.flash_size_arg()])
        .arg("--merge")
        .arg("--skip-padding")
        .arg(elf)
        .arg(image_path)
        .status()
        .context("running espflash save-image (is espflash installed?)")?;
    if !status.success() {
        bail!("espflash save-image failed for firmware build `{}`", def.id);
    }
    Ok(())
}

/// Drop images and manifests from a previous packaging run so a failed merge
/// cannot leave a stale image next to a fresh manifest.
fn remove_stale_outputs(out_dir: &Path) -> Result<()> {
    for entry in std::fs::read_dir(out_dir)? {
        let path = entry?.path();
        let is_stale = path.extension().is_some_and(|ext| ext == "bin")
            || path.file_name().is_some_and(|name| name == "manifest.json");
        if is_stale {
            std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
        }
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use lpc_model::manifest::{MANIFEST_BLOB_BEGIN, MANIFEST_BLOB_END};

    use super::*;

    const CORE_JSON: &str = r#"{"lpManifestCore":1,"package":"fw-esp32c6",
        "profile":"release-esp32","commit":"abc123456789","dirty":false,
        "target":{"family":"esp32","chip":"esp32c6",
        "cargoTarget":"riscv32imac-unknown-none-elf"},
        "features":["node.shader","gfx.lpvm"],
        "limits":{"flashAppBytes":3145728},"wireProto":4}"#;

    fn artifact_with(core: &str) -> Vec<u8> {
        let mut bytes = vec![0u8; 32];
        bytes.extend_from_slice(MANIFEST_BLOB_BEGIN.as_bytes());
        bytes.extend_from_slice(core.as_bytes());
        bytes.extend_from_slice(MANIFEST_BLOB_END.as_bytes());
        bytes.extend_from_slice(&[0xFFu8; 32]);
        bytes
    }

    #[test]
    fn extracts_core_as_raw_json_and_parsed() {
        let bytes = artifact_with(CORE_JSON);
        let (raw, core) = extract_core(&bytes, Path::new("fake.elf")).unwrap();
        assert_eq!(core.package, "fw-esp32c6");
        assert_eq!(core.wire_proto, 4);
        // Verbatim: every key the build emitted survives into the manifest.
        assert_eq!(raw["package"], "fw-esp32c6");
        assert_eq!(raw["limits"]["flashAppBytes"], 3_145_728);
        assert_eq!(raw["features"][0], "node.shader");
    }

    /// The drift assertion compares parsed cores, so two artifacts built from
    /// different sources are caught even when their JSON only differs in
    /// whitespace-insensitive ways.
    #[test]
    fn differing_cores_compare_unequal() {
        let a = artifact_with(CORE_JSON);
        let b = artifact_with(&CORE_JSON.replace("abc123456789", "def987654321"));
        let (_, core_a) = extract_core(&a, Path::new("a.elf")).unwrap();
        let (_, core_b) = extract_core(&b, Path::new("b.bin")).unwrap();
        assert_ne!(core_a, core_b);
    }

    #[test]
    fn missing_core_is_an_error() {
        let error = extract_core(&[0u8; 64], Path::new("empty.bin")).unwrap_err();
        assert!(error.to_string().contains("no firmware manifest core"));
    }

    /// Identity guard: the checked-in C6 def accepts a C6 core and rejects
    /// an artifact from another variant.
    #[test]
    fn core_must_match_the_build_def() {
        let repo_root = find_repo_root().unwrap();
        let def = load_build_def(&repo_root, "esp32c6-4mb").unwrap();
        let (_, core) = extract_core(&artifact_with(CORE_JSON), Path::new("c6.elf")).unwrap();
        check_core_matches_def(&def, &core, Path::new("c6.elf")).unwrap();

        let foreign = CORE_JSON.replace("esp32c6", "esp32s3");
        let (_, core) = extract_core(&artifact_with(&foreign), Path::new("s3.elf")).unwrap();
        let error = check_core_matches_def(&def, &core, Path::new("s3.elf"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("chip.name"), "{error}");
    }

    #[test]
    fn sha256_is_lowercase_hex() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
