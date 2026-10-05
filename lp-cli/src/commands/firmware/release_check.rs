//! `lp-cli firmware release-check <dir>` — re-verify a release staging
//! directory **from its files alone**: what the release workflow runs right
//! before it uploads, and what a person runs on a downloaded release.
//!
//! For every target: the package manifest and its images, hashes matching.
//! For a split package also its `ota-manifest.json`: valid against the
//! checked-in schema (embedded, so no checkout is needed), parsed and
//! validated by `lpc-firmware-release`, every file it names present with
//! the right length and hash, encoding 1's chunks proven with the one
//! packer's prover (`lpa_update::pack::prove_piece`), the core's digest slot
//! holding the engine's hash, the identity agreeing with the package's
//! manifest core, and `requires` naming the code table's layout and loader.
//! A file nobody accounts for is an error, and so is a dev version unless
//! `--allow-dev`.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use lpa_update::EncodedPiece;
use lpa_update::pack::{ProveError, prove_piece};
use lpc_firmware_release::{
    OTA_MANIFEST_FILE, OtaManifest, TargetName, asset_name, sha256_hex, split_asset_name,
};
use lpc_update::PieceKind;
use lpc_update::code_table::{CHUNK, LAYOUT_1, LOADER_1};
use lpc_update::dictionary_rule::WINDOW;
use serde_json::Value;

use super::args::ReleaseCheckArgs;
use super::build_def::{find_repo_root, load_served_targets};
use super::distribution_manifest::DistributionManifest;
use super::ota_files::{CoreIdentity, PACKAGE_JSON, requires_of};
use super::release_assets::check_version;
use super::split_package::check_core_carries_engine_digest;

/// `schemas/ota-manifest.schema.json`, as this binary was built with it.
const OTA_MANIFEST_SCHEMA: &str = include_str!("../../../../schemas/ota-manifest.schema.json");

pub fn handle_release_check(args: ReleaseCheckArgs) -> Result<()> {
    let targets = match args.targets {
        Some(targets) => targets,
        None => match find_repo_root() {
            Ok(repo_root) => load_served_targets(&repo_root)?,
            // Outside a checkout (a downloaded release): every target the
            // directory holds a package for.
            Err(_) => targets_in(&args.dir)?,
        },
    };
    let checked = check_release_dir(&args.dir, &targets, args.allow_dev)?;
    println!(
        "release-check: {} files in {} verified for {}",
        checked,
        args.dir.display(),
        targets.join(", ")
    );
    Ok(())
}

/// Check `dir` for `targets`. Returns how many files were verified, or every
/// problem found, one per line.
pub fn check_release_dir(dir: &Path, targets: &[String], allow_dev: bool) -> Result<usize> {
    ensure!(!targets.is_empty(), "no targets to check");
    let mut accounted = BTreeSet::new();
    let mut problems = Vec::new();
    for target in targets {
        if let Err(e) = check_target(dir, target, allow_dev, &mut accounted) {
            problems.push(format!("{target}: {e:#}"));
        }
    }
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if !accounted.contains(&name) {
            problems.push(format!("{name}: not accounted for by any checked target"));
        }
    }
    if !problems.is_empty() {
        bail!(
            "{} is not a valid release ({} problems):\n  {}",
            dir.display(),
            problems.len(),
            problems.join("\n  ")
        );
    }
    Ok(accounted.len())
}

/// The schema's complaints about an `ota-manifest.json` value, if any.
pub fn schema_errors(value: &Value) -> Result<()> {
    let schema: Value =
        serde_json::from_str(OTA_MANIFEST_SCHEMA).context("the embedded schema is not JSON")?;
    let validator = jsonschema::draft202012::new(&schema)
        .map_err(|e| anyhow::anyhow!("the embedded schema does not compile: {e}"))?;
    let errors: Vec<String> = validator
        .iter_errors(value)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        bail!(
            "does not match schemas/ota-manifest.schema.json: {}",
            errors.join("; ")
        )
    }
}

fn check_target(
    dir: &Path,
    target: &str,
    allow_dev: bool,
    accounted: &mut BTreeSet<String>,
) -> Result<()> {
    let name = TargetName::parse(target).with_context(|| format!("`{target}` is not a target"))?;
    let mut read = |file: &str| -> Result<Vec<u8>> {
        let asset = asset_name(&name, file);
        let bytes =
            std::fs::read(dir.join(&asset)).with_context(|| format!("{asset} is missing"))?;
        accounted.insert(asset);
        Ok(bytes)
    };

    let package_bytes = read(PACKAGE_JSON)?;
    let package = DistributionManifest::parse(&package_bytes)?;
    ensure!(
        package.firmware_id == target,
        "{target}.{PACKAGE_JSON} is the package of `{}`",
        package.firmware_id
    );
    let identity = CoreIdentity::read(&package)?;
    check_version(&identity.version, allow_dev)?;
    let mut image = None;
    for entry in &package.images {
        let bytes = read(&entry.path)?;
        ensure!(
            bytes.len() as u64 == entry.size_bytes && sha256_hex(&bytes) == entry.sha256,
            "{target}.{}: {} bytes, sha256 {}; the package manifest says {} bytes, sha256 {}",
            entry.path,
            bytes.len(),
            sha256_hex(&bytes),
            entry.size_bytes,
            entry.sha256
        );
        image = Some((entry.path.clone(), bytes));
    }

    let Some(split) = &package.split else {
        return Ok(());
    };
    let [_] = package.images.as_slice() else {
        bail!("a split package flashes one merged image");
    };
    let (image_name, image) = image.expect("one image was read");

    let ota_bytes = read(OTA_MANIFEST_FILE)?;
    let value: Value =
        serde_json::from_slice(&ota_bytes).context("the OTA manifest is not JSON")?;
    schema_errors(&value)?;
    let ota = OtaManifest::parse_valid(&ota_bytes).map_err(|e| anyhow::anyhow!("{e}"))?;

    // Every file it names, by length and hash.
    let mut files = std::collections::BTreeMap::new();
    for file in ota.files() {
        let bytes = read(file.file)?;
        ota.verify(file.file, &bytes)?;
        files.insert(file.file.to_string(), bytes);
    }
    ensure!(
        ota.package.file == PACKAGE_JSON && ota.package.image.file == image_name,
        "the OTA manifest names package `{}` and image `{}`, not this package's",
        ota.package.file,
        ota.package.image.file
    );

    // Identity: the OTA manifest, the package's manifest core and the split
    // block tell one story.
    for (field, ota_value, core_value) in [
        ("target", ota.target.as_str(), identity.target.as_str()),
        ("chip", ota.chip.as_str(), identity.chip.as_str()),
        ("version", ota.version.as_str(), identity.version.as_str()),
    ] {
        ensure!(
            ota_value == core_value,
            "{field}: the OTA manifest says `{ota_value}`, the package's manifest core `{core_value}`"
        );
    }
    ensure!(
        ota.target == target,
        "the OTA manifest is for `{}`",
        ota.target
    );
    ensure!(
        ota.wire_proto == identity.wire_proto,
        "wireProto: the OTA manifest says {}, the package's manifest core {}",
        ota.wire_proto,
        identity.wire_proto
    );
    ensure!(
        ota.commit.starts_with(&identity.commit),
        "commit: the OTA manifest's {} does not extend the image's {}",
        ota.commit,
        identity.commit
    );
    ensure!(
        split.build_id == ota.build_id(),
        "the split block's build id `{}` is not the OTA manifest's `{}`",
        split.build_id,
        ota.build_id()
    );

    // `requires`: the code table's layout and loader, and the image's own.
    let expected = requires_of(split, &image)?;
    ensure!(
        ota.requires == expected && (expected.layout, expected.loader) == (LAYOUT_1, LOADER_1),
        "requires is {{layout {}, loader {}}}, the image and the code table say {{layout {}, loader {}}}",
        ota.requires.layout,
        ota.requires.loader,
        expected.layout,
        expected.loader
    );

    // The pieces are the image's bytes where the split block places them,
    // and the core's digest slot holds the engine's hash.
    let core = &files[&ota.core.file];
    let engine = &files[&ota.engine.file];
    for (field, piece, bytes) in [
        ("core", &split.core, core),
        ("engine", &split.engine, engine),
    ] {
        ensure!(
            piece.slice(&image)? == bytes.as_slice() && piece.sha256 == sha256_hex(bytes),
            "{field}: the OTA manifest's piece is not the image's bytes at {}",
            piece.offset
        );
    }
    check_core_carries_engine_digest(core, engine)?;

    // Encoding 1's chunks, proven by the one packer's prover. Other ids are
    // skipped.
    if let Some(e) = ota.encoding1() {
        ensure!(
            e.chunk_bytes == CHUNK && e.window_bytes == WINDOW,
            "encoding 1 says {} B chunks and a {} B window; the dictionary rule is {CHUNK} and {WINDOW}",
            e.chunk_bytes,
            e.window_bytes
        );
        for (kind, raw, z) in [
            (PieceKind::Core, core, &e.core),
            (PieceKind::Engine, engine, &e.engine),
        ] {
            let encoded = EncodedPiece {
                stream: files[&z.file].clone(),
                chunks: z.chunks.clone(),
            };
            prove_piece(kind, raw, &encoded).map_err(|err| match err {
                ProveError::Shape => anyhow::anyhow!("{}: the index does not describe it", z.file),
                ProveError::Chunk { idx } => anyhow::anyhow!(
                    "{}: chunk {idx} does not decode to the piece's bytes",
                    z.file
                ),
            })?;
        }
    }
    Ok(())
}

/// The targets a directory holds packages for (`<target>.package.json`).
fn targets_in(dir: &Path) -> Result<Vec<String>> {
    let mut targets = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if let Some((target, PACKAGE_JSON)) = split_asset_name(&name) {
            targets.push(target.as_str().to_string());
        }
    }
    targets.sort();
    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::super::ota_fixture::{Fixture, IMAGE, TARGET};
    use super::super::release_assets::{PackageSources, stage_release_assets};
    use super::*;
    use std::path::PathBuf;

    /// A release staged from a fresh fixture: the C6 (split) and an S3.
    fn staged(version: &str) -> (Fixture, PathBuf, Vec<String>) {
        let fx = Fixture::new(version);
        fx.write().unwrap().unwrap();
        fx.add_plain_package("esp32s3-8mb", "esp32s3", version);
        let out = fx.root.join("stage");
        let targets = vec![TARGET.to_string(), "esp32s3-8mb".to_string()];
        let sources = PackageSources {
            packages_root: fx.packages_root(),
            parts_root: fx.parts_root(),
        };
        stage_release_assets(&sources, &targets, &out, true).unwrap();
        (fx, out, targets)
    }

    fn refused(dir: &Path, targets: &[String], allow_dev: bool) -> String {
        format!(
            "{:#}",
            check_release_dir(dir, targets, allow_dev).unwrap_err()
        )
    }

    fn edit_json(path: &Path, edit: impl FnOnce(&mut Value)) {
        let mut value: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        edit(&mut value);
        std::fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    }

    #[test]
    fn a_staged_release_passes() {
        let (_fx, out, targets) = staged("2026.10.05-3");
        assert_eq!(check_release_dir(&out, &targets, false).unwrap(), 9);
        assert_eq!(targets_in(&out).unwrap(), ["esp32c6-4mb", "esp32s3-8mb"]);
    }

    #[test]
    fn a_flipped_byte_in_engine_z_is_refused() {
        let (_fx, out, targets) = staged("2026.10.05-3");
        let path = out.join(format!("{TARGET}.engine.z"));
        let mut z = std::fs::read(&path).unwrap();
        z[40] ^= 0x10;
        std::fs::write(&path, z).unwrap();
        let error = refused(&out, &targets, false);
        assert!(error.contains("engine.z: sha256"), "{error}");
    }

    /// A tamper that also fixes up the manifest's hash still fails, on the
    /// chunk itself: the prover decodes every chunk.
    #[test]
    fn a_rehashed_bad_chunk_is_refused_by_the_prover() {
        let (_fx, out, targets) = staged("2026.10.05-3");
        let z_path = out.join(format!("{TARGET}.engine.z"));
        let ota_path = out.join(format!("{TARGET}.ota-manifest.json"));
        let ota = OtaManifest::parse(&std::fs::read(&ota_path).unwrap()).unwrap();
        let e = ota.encoding1().unwrap();
        let first = e.engine.chunks.iter().position(|c| *c > 0).unwrap();
        let range = e.engine.chunk_range(first).unwrap();
        let mut z = std::fs::read(&z_path).unwrap();
        let mid = (range.start + range.end) as usize / 2;
        z[mid] ^= 0xff;
        std::fs::write(&z_path, &z).unwrap();
        edit_json(&ota_path, |m| {
            m["encodings"][0]["engine"]["sha256"] = sha256_hex(&z).into();
        });
        let error = refused(&out, &targets, false);
        assert!(
            error.contains(&format!("engine.z: chunk {first} does not decode")),
            "{error}"
        );
    }

    /// A release whose every hash is consistent but whose core's digest
    /// slot does not hold the engine's hash. The writer refuses to make one,
    /// so it is assembled here: the bad fixture's package, image and core,
    /// with the core re-packed and the OTA manifest's hashes fixed up.
    #[test]
    fn a_wrong_digest_slot_is_refused() {
        let version = "2026.10.05-3";
        let bad = Fixture::with_wrong_digest_slot(version);
        assert!(format!("{:#}", bad.write().unwrap_err()).contains("engine digest slot"));
        let (_good, out, targets) = staged(version);
        let asset = |file: &str| out.join(format!("{TARGET}.{file}"));

        let package = std::fs::read(bad.package_dir().join("manifest.json")).unwrap();
        let image = std::fs::read(bad.package_dir().join(IMAGE)).unwrap();
        let core_z = lpa_update::pack::pack_piece(PieceKind::Core, &bad.core);
        std::fs::write(asset(PACKAGE_JSON), &package).unwrap();
        std::fs::write(asset(IMAGE), &image).unwrap();
        std::fs::write(asset("core.bin"), &bad.core).unwrap();
        std::fs::write(asset("core.z"), &core_z.stream).unwrap();
        edit_json(&asset(OTA_MANIFEST_FILE), |m| {
            m["core"]["sha256"] = sha256_hex(&bad.core).into();
            let z = &mut m["encodings"][0]["core"];
            z["length"] = core_z.stream.len().into();
            z["sha256"] = sha256_hex(&core_z.stream).into();
            z["chunks"] = core_z.chunks.clone().into();
            m["package"]["length"] = package.len().into();
            m["package"]["sha256"] = sha256_hex(&package).into();
            m["package"]["image"]["sha256"] = sha256_hex(&image).into();
        });
        let error = refused(&out, &targets, false);
        assert!(
            error.contains("the core's engine digest slot holds"),
            "{error}"
        );
    }

    #[test]
    fn a_version_or_wire_proto_disagreement_is_refused() {
        let (_fx, out, targets) = staged("2026.10.05-3");
        let ota_path = out.join(format!("{TARGET}.ota-manifest.json"));
        edit_json(&ota_path, |m| m["wireProto"] = 35.into());
        let error = refused(&out, &targets, false);
        assert!(
            error.contains("wireProto: the OTA manifest says 35"),
            "{error}"
        );

        let (_fx, out, targets) = staged("2026.10.05-3");
        let ota_path = out.join(format!("{TARGET}.ota-manifest.json"));
        edit_json(&ota_path, |m| m["version"] = "2026.10.05-4".into());
        let error = refused(&out, &targets, false);
        assert!(
            error.contains("version: the OTA manifest says `2026.10.05-4`"),
            "{error}"
        );
    }

    #[test]
    fn an_extra_file_is_refused() {
        let (_fx, out, targets) = staged("2026.10.05-3");
        std::fs::write(out.join("esp32c6-4mb.notes.txt"), b"hi").unwrap();
        let error = refused(&out, &targets, false);
        assert!(
            error.contains("esp32c6-4mb.notes.txt: not accounted for"),
            "{error}"
        );
    }

    #[test]
    fn a_missing_package_image_is_refused() {
        let (_fx, out, targets) = staged("2026.10.05-3");
        std::fs::remove_file(out.join("esp32s3-8mb.fw-esp32s3-merged.bin")).unwrap();
        let error = refused(&out, &targets, false);
        assert!(
            error.contains("esp32s3-8mb.fw-esp32s3-merged.bin is missing"),
            "{error}"
        );
    }

    #[test]
    fn a_dev_version_needs_allow_dev() {
        let (_fx, out, targets) = staged("abc1234-dirty-101500PT");
        let error = refused(&out, &targets, false);
        assert!(error.contains("dev version"), "{error}");
        check_release_dir(&out, &targets, true).unwrap();
    }

    #[test]
    fn a_schema_violation_is_refused() {
        let (_fx, out, targets) = staged("2026.10.05-3");
        let ota_path = out.join(format!("{TARGET}.ota-manifest.json"));
        edit_json(&ota_path, |m| m["commit"] = "ABC".into());
        let error = refused(&out, &targets, false);
        assert!(
            error.contains("schemas/ota-manifest.schema.json"),
            "{error}"
        );
    }

    #[test]
    fn requires_must_name_the_code_tables_layout_and_loader() {
        let (_fx, out, targets) = staged("2026.10.05-3");
        let ota_path = out.join(format!("{TARGET}.ota-manifest.json"));
        edit_json(&ota_path, |m| m["requires"]["loader"] = 0.into());
        let error = refused(&out, &targets, false);
        assert!(
            error.contains("requires is {layout 1, loader 0}"),
            "{error}"
        );
    }
}
