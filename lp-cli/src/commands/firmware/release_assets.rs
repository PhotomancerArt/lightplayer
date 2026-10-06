//! `lp-cli firmware release-assets` — gather what `firmware package` wrote
//! into a release staging directory, under the release's asset names
//! (`<target>.<file>`, one-way-doors §15). It **compresses nothing**: the
//! `.z` files and `ota-manifest.json` are the package step's, and every file
//! is verified against the manifest that names it before it is staged.
//!
//! Per target: the package manifest as `<target>.package.json` (its bytes
//! verbatim) and each image as `<target>.<image>`; for a split package also
//! `<target>.ota-manifest.json` and every file it names. Nothing here
//! uploads; the release workflow does (a later phase).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use lpc_firmware_release::{
    OTA_MANIFEST_FILE, OtaManifest, ReleaseVersion, TargetName, asset_name, is_app_version,
    sha256_hex,
};

use super::args::ReleaseAssetsArgs;
use super::build_def::{find_repo_root, load_served_targets};
use super::distribution_manifest::DistributionManifest;
use super::ota_files::{PACKAGE_JSON, PACKAGE_MANIFEST_FILE};
use super::package::DEFAULT_OUT_ROOT;
use super::split_package::PARTS_ROOT;

/// Where the packages and their OTA directories are.
pub struct PackageSources {
    /// `<packages_root>/<target>/`: `manifest.json` and the images.
    pub packages_root: PathBuf,
    /// `<parts_root>/<target>/`: a split package's OTA directory.
    pub parts_root: PathBuf,
}

impl PackageSources {
    /// The checkout's: `firmware package`'s default output and parts roots.
    pub fn repo(repo_root: &Path) -> Self {
        Self {
            packages_root: repo_root.join(DEFAULT_OUT_ROOT),
            parts_root: repo_root.join(PARTS_ROOT),
        }
    }
}

pub fn handle_release_assets(args: ReleaseAssetsArgs) -> Result<()> {
    let repo_root = find_repo_root()?;
    let targets = match args.targets {
        Some(targets) => targets,
        None => load_served_targets(&repo_root)?,
    };
    let staged = stage_release_assets(
        &PackageSources::repo(&repo_root),
        &targets,
        &args.out,
        args.allow_dev,
    )?;
    println!("staged {} assets in {}:", staged.len(), args.out.display());
    for (name, len) in &staged {
        println!("  {len:>10}  {name}");
    }
    Ok(())
}

/// Stage `targets` from `sources` into `out` (absent or empty), returning
/// each asset name with its length.
pub fn stage_release_assets(
    sources: &PackageSources,
    targets: &[String],
    out: &Path,
    allow_dev: bool,
) -> Result<BTreeMap<String, u64>> {
    ensure!(!targets.is_empty(), "no targets to stage");
    if out.exists() {
        let mut entries =
            std::fs::read_dir(out).with_context(|| format!("reading {}", out.display()))?;
        ensure!(
            entries.next().is_none(),
            "{} is not empty: a staging directory starts empty, so nothing stale ships",
            out.display()
        );
    }
    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;

    let mut staged = BTreeMap::new();
    for target in targets {
        stage_target(sources, target, out, allow_dev, &mut staged)
            .with_context(|| format!("staging {target}"))?;
    }
    Ok(staged)
}

fn stage_target(
    sources: &PackageSources,
    target: &str,
    out: &Path,
    allow_dev: bool,
    staged: &mut BTreeMap<String, u64>,
) -> Result<()> {
    let name = TargetName::parse(target).with_context(|| format!("`{target}` is not a target"))?;
    let package_dir = sources.packages_root.join(target);
    let read = |dir: &Path, file: &str| -> Result<Vec<u8>> {
        let path = dir.join(file);
        std::fs::read(&path).with_context(|| {
            format!(
                "reading {} — run `lp-cli firmware package {target}` first",
                path.display()
            )
        })
    };
    let mut stage = |file: &str, bytes: &[u8]| -> Result<()> {
        let asset = asset_name(&name, file);
        let path = out.join(&asset);
        ensure!(!path.exists(), "{asset} would be staged twice");
        std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
        staged.insert(asset, bytes.len() as u64);
        Ok(())
    };

    let package_bytes = read(&package_dir, PACKAGE_MANIFEST_FILE)?;
    let package = DistributionManifest::parse(&package_bytes)?;
    ensure!(
        package.firmware_id == target,
        "the package in {} is `{}`",
        package_dir.display(),
        package.firmware_id
    );
    let version = package
        .core_str("version")
        .context("the package's manifest core has no `version`")?;
    check_version(version, allow_dev)?;

    stage(PACKAGE_JSON, &package_bytes)?;
    let mut images = BTreeMap::new();
    for image in &package.images {
        let bytes = read(&package_dir, &image.path)?;
        ensure!(
            bytes.len() as u64 == image.size_bytes && sha256_hex(&bytes) == image.sha256,
            "{} is not the image the package manifest describes",
            image.path
        );
        stage(&image.path, &bytes)?;
        images.insert(image.path.clone(), bytes);
    }

    if package.split.is_none() {
        return Ok(());
    }
    let ota_dir = sources.parts_root.join(target);
    let ota_path = ota_dir.join(OTA_MANIFEST_FILE);
    let ota_bytes = std::fs::read(&ota_path).with_context(|| {
        format!(
            "{} is missing: a split package's OTA files are written by `lp-cli firmware \
             package {target}`",
            ota_path.display()
        )
    })?;
    let ota = OtaManifest::parse_valid(&ota_bytes)
        .map_err(|e| anyhow::anyhow!("{}: {e}", ota_path.display()))?;
    ensure!(
        ota.target == target && ota.version == version,
        "{} is for {} {}, not this package's {target} {version}",
        ota_path.display(),
        ota.target,
        ota.version
    );
    ensure!(
        ota.package.file == PACKAGE_JSON,
        "the OTA manifest names its package `{}`, not `{PACKAGE_JSON}`",
        ota.package.file
    );
    stage(OTA_MANIFEST_FILE, &ota_bytes)?;
    for file in ota.files() {
        // The package manifest and its image are staged above, from the
        // package; the OTA manifest must name exactly those bytes.
        let bytes = if file.file == ota.package.file {
            package_bytes.clone()
        } else if let Some(image) = images.get(file.file) {
            image.clone()
        } else {
            let bytes = read(&ota_dir, file.file)?;
            ota.verify(file.file, &bytes)?;
            stage(file.file, &bytes)?;
            continue;
        };
        ota.verify(file.file, &bytes).with_context(|| {
            format!(
                "the OTA manifest's `{}` is not the staged package's",
                file.file
            )
        })?;
    }
    ensure!(
        images.contains_key(&ota.package.image.file),
        "the OTA manifest's image `{}` is not one of the package's images",
        ota.package.image.file
    );
    Ok(())
}

/// A release stages release versions only; `allow_dev` (the pre-merge dry
/// run) also takes a dev version.
pub fn check_version(version: &str, allow_dev: bool) -> Result<()> {
    if ReleaseVersion::parse(version).is_some() {
        return Ok(());
    }
    if allow_dev && is_app_version(version) {
        return Ok(());
    }
    if is_app_version(version) {
        bail!(
            "{version} is a dev version: a release carries release versions (--allow-dev for a dry run)"
        );
    }
    bail!("`{version}` is not an app version");
}

#[cfg(test)]
mod tests {
    use super::super::ota_fixture::{Fixture, IMAGE, TARGET};
    use super::*;

    fn sources(fx: &Fixture) -> PackageSources {
        PackageSources {
            packages_root: fx.packages_root(),
            parts_root: fx.parts_root(),
        }
    }

    #[test]
    fn stages_every_file_under_its_asset_name_and_compresses_nothing() {
        let fx = Fixture::new("2026.10.05-3");
        fx.write().unwrap().unwrap();
        fx.add_plain_package("esp32s3-8mb", "esp32s3", "2026.10.05-3");
        let out = fx.root.join("stage");
        let staged = stage_release_assets(
            &sources(&fx),
            &[TARGET.to_string(), "esp32s3-8mb".to_string()],
            &out,
            false,
        )
        .unwrap();
        let names: Vec<&str> = staged.keys().map(String::as_str).collect();
        assert_eq!(
            names,
            [
                "esp32c6-4mb.core.bin",
                "esp32c6-4mb.core.z",
                "esp32c6-4mb.engine.bin",
                "esp32c6-4mb.engine.z",
                "esp32c6-4mb.fw-esp32c6-merged.bin",
                "esp32c6-4mb.ota-manifest.json",
                "esp32c6-4mb.package.json",
                "esp32s3-8mb.fw-esp32s3-merged.bin",
                "esp32s3-8mb.package.json",
            ]
        );
        // Byte copies: the .z streams are the package step's, not re-made.
        for (asset, from) in [
            ("core.z", fx.ota_dir().join("core.z")),
            ("ota-manifest.json", fx.ota_dir().join("ota-manifest.json")),
            ("package.json", fx.package_dir().join("manifest.json")),
            (IMAGE, fx.package_dir().join(IMAGE)),
        ] {
            assert_eq!(
                std::fs::read(out.join(format!("{TARGET}.{asset}"))).unwrap(),
                std::fs::read(from).unwrap(),
                "{asset}"
            );
        }
    }

    #[test]
    fn a_non_split_target_stages_only_its_package() {
        let fx = Fixture::new("2026.10.05-3");
        fx.add_plain_package("esp32v3-4mb", "esp32", "2026.10.05-3");
        let out = fx.root.join("stage");
        let staged =
            stage_release_assets(&sources(&fx), &["esp32v3-4mb".to_string()], &out, false).unwrap();
        assert_eq!(
            staged.keys().collect::<Vec<_>>(),
            [
                "esp32v3-4mb.fw-esp32-merged.bin",
                "esp32v3-4mb.package.json"
            ]
        );
    }

    #[test]
    fn a_split_target_without_ota_files_names_the_package_step() {
        let fx = Fixture::new("2026.10.05-3");
        let error = stage_release_assets(
            &sources(&fx),
            &[TARGET.to_string()],
            &fx.root.join("stage"),
            false,
        )
        .unwrap_err();
        let error = format!("{error:#}");
        assert!(
            error.contains("lp-cli firmware package esp32c6-4mb"),
            "{error}"
        );
    }

    #[test]
    fn dev_versions_need_allow_dev_and_the_out_dir_must_be_empty() {
        let fx = Fixture::new("abc1234-dirty-101500PT");
        fx.write().unwrap().unwrap();
        let out = fx.root.join("stage");
        let error =
            stage_release_assets(&sources(&fx), &[TARGET.to_string()], &out, false).unwrap_err();
        assert!(format!("{error:#}").contains("dev version"), "{error:#}");
        std::fs::remove_dir_all(&out).unwrap();
        stage_release_assets(&sources(&fx), &[TARGET.to_string()], &out, true).unwrap();
        let error =
            stage_release_assets(&sources(&fx), &[TARGET.to_string()], &out, true).unwrap_err();
        assert!(error.to_string().contains("not empty"), "{error}");
    }

    #[test]
    fn a_tampered_ota_file_is_not_staged() {
        let fx = Fixture::new("2026.10.05-3");
        fx.write().unwrap().unwrap();
        let path = fx.ota_dir().join("engine.z");
        let mut z = std::fs::read(&path).unwrap();
        z[10] ^= 1;
        std::fs::write(&path, z).unwrap();
        let error = stage_release_assets(
            &sources(&fx),
            &[TARGET.to_string()],
            &fx.root.join("stage"),
            false,
        )
        .unwrap_err();
        assert!(
            format!("{error:#}").contains("engine.z: sha256"),
            "{error:#}"
        );
    }
}
