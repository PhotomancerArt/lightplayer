//! `--ota-offer <dir>`: the build in an OTA directory, read through its
//! `ota-manifest.json` with `lpa_update`'s `HostBuild::from_ota_manifest` —
//! never the package's `split` block (one-way-doors §4).
//!
//! Two layouts are accepted, through one injected reader: a package's parts
//! directory (`ota-manifest.json`, `core.bin`, …, as `lp-cli firmware
//! package` writes it) and a release staging directory (the same files named
//! `<target>.<file>`, as `release-assets` stages them).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lpa_update::HostBuild;
use lpc_firmware_release::{OTA_MANIFEST_FILE, OtaManifest};

/// The release in `dir` and the build it describes. `no_z` drops its
/// encodings, so every chunk goes raw.
pub fn load_offer(dir: &Path, no_z: bool) -> Result<(OtaManifest, HostBuild)> {
    let manifest_path = manifest_path(dir)?;
    let bytes = std::fs::read(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let manifest = OtaManifest::parse_valid(&bytes)
        .map_err(|e| anyhow::anyhow!("{}: {e}", manifest_path.display()))?;
    let mut served = manifest.clone();
    if no_z {
        served.encodings.clear();
    }
    let target = manifest.target.clone();
    let read = |name: &str| -> Option<Vec<u8>> {
        std::fs::read(dir.join(name))
            .or_else(|_| std::fs::read(dir.join(format!("{target}.{name}"))))
            .ok()
    };
    let build = HostBuild::from_ota_manifest(&served, read).map_err(|e| {
        anyhow::anyhow!(
            "{} does not describe the files beside it: {e:?}",
            manifest_path.display()
        )
    })?;
    Ok((manifest, build))
}

/// `dir/ota-manifest.json`, or the one `<target>.ota-manifest.json` there.
fn manifest_path(dir: &Path) -> Result<PathBuf> {
    let plain = dir.join(OTA_MANIFEST_FILE);
    if plain.is_file() {
        return Ok(plain);
    }
    let suffix = format!(".{OTA_MANIFEST_FILE}");
    let staged: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading the OTA directory {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(&suffix))
        })
        .collect();
    match staged.as_slice() {
        [one] => Ok(one.clone()),
        [] => bail!(
            "{} has no {OTA_MANIFEST_FILE}: it is not a split package's OTA directory (or it \
             was packaged before packages carried their OTA files). `lp-cli firmware package \
             <target>` writes it beside core.bin and engine.bin in \
             target/firmware-parts/<target>/",
            dir.display()
        ),
        many => bail!(
            "{} holds {} staged manifests; offer one target's directory",
            dir.display(),
            many.len()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directory_without_a_manifest_says_which_file_it_lacks() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("core.bin"), b"x").unwrap();
        let err = load_offer(tmp.path(), false).unwrap_err().to_string();
        assert!(err.contains("ota-manifest.json"), "{err}");
    }

    #[test]
    fn a_packaged_ota_directory_is_offered_with_its_encoding() {
        let fx = crate::commands::firmware::ota_fixture::Fixture::new("abc1234");
        fx.write().unwrap().expect("OTA files written");
        let (manifest, build) = load_offer(&fx.ota_dir(), false).unwrap();
        assert_eq!(build.identity.build_id, manifest.build_id());
        assert_eq!(build.core.bytes, fx.core);
        assert!(build.core.encoded.is_some() && build.engine.encoded.is_some());
        let (_, raw) = load_offer(&fx.ota_dir(), true).unwrap();
        assert!(raw.core.encoded.is_none() && raw.engine.encoded.is_none());
        assert_eq!(raw.core.sha256, build.core.sha256);
    }

    #[test]
    fn a_release_staging_directory_is_read_by_its_asset_names() {
        let fx = crate::commands::firmware::ota_fixture::Fixture::new("abc1234");
        fx.write().unwrap().expect("OTA files written");
        let staged = tempfile::tempdir().unwrap();
        for entry in std::fs::read_dir(fx.ota_dir()).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap().to_string();
            std::fs::copy(&path, staged.path().join(format!("esp32c6-4mb.{name}"))).unwrap();
        }
        let (manifest, _) = load_offer(staged.path(), false).unwrap();
        assert_eq!(manifest.target, "esp32c6-4mb");
    }
}
