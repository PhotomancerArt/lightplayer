//! The OTA files of a split package: `ota-manifest.json`, `core.z` and
//! `engine.z`, written by `lp-cli firmware package` into the package's
//! **OTA directory** (the split parts directory, beside `core.bin` and
//! `engine.bin`; never the packaged directory the Studio bundle copies).
//! Local packages, emulator offers (`--ota-offer`), deploys and releases
//! then hold the same files (one-way-doors §4); `release-assets` only
//! renames them.
//!
//! Every identity field is **read from the image, never re-typed**: the
//! package manifest's `core` block is the image's manifest core verbatim
//! (the packager checked that in the same run), and the `split` block is
//! the split tool's account of the same bytes. The one fact the image does
//! not carry whole is the commit: the manifest core holds its first 12
//! digits, and the full 40 are resolved from the checkout that built it.
//!
//! The compression is **the one packer**, `lpa_update::pack::pack_piece`
//! (encoding 1, the dictionary rule in `lpc_update`); lp-cli compresses
//! nothing itself. The output is a function of the inputs: no timestamp, so
//! the same package gives the same bytes.

use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use lpa_update::EncodedPiece;
use lpa_update::pack::pack_piece;
use lpc_firmware_release::{
    COMMIT_HEX_LEN, EncodedPieceFile, Encoding1, EncodingEntry, OTA_MANIFEST_FILE,
    OTA_MANIFEST_FORMAT, OtaManifest, PackageRef, PieceFile, Requires, is_lower_hex,
    is_release_version, sha256_hex,
};
use lpc_update::PieceKind;
use lpc_update::code_table::{CHUNK, ENCODING_1, LAYOUT_1, LOADER_1};
use lpc_update::dictionary_rule::WINDOW;

use super::build_def::BuildDef;
use super::distribution_manifest::{DistributionManifest, SplitBlock};
use super::split_package::check_core_carries_engine_digest;

/// `core.bin` inside the OTA directory and the release.
pub const CORE_BIN: &str = "core.bin";
/// `engine.bin`, exactly as flashed.
pub const ENGINE_BIN: &str = "engine.bin";
/// Encoding 1's stream of the core.
pub const CORE_Z: &str = "core.z";
/// Encoding 1's stream of the engine.
pub const ENGINE_Z: &str = "engine.z";
/// The package manifest's name inside a release (the packaged directory
/// calls it `manifest.json`; a release stages it as `<target>.package.json`).
pub const PACKAGE_JSON: &str = "package.json";
/// The package manifest's name inside the packaged directory.
pub const PACKAGE_MANIFEST_FILE: &str = "manifest.json";
/// Encoding 1's codec word, for people and tools (readers choose by `id`).
pub const ENCODING_1_CODEC: &str = "deflate-raw";

/// What [`write_ota_files`] wrote, for the summary.
#[derive(Debug)]
pub struct OtaFiles {
    pub manifest: OtaManifest,
    pub core: EncodedPiece,
    pub engine: EncodedPiece,
}

/// Resolves a manifest core's short commit to the full 40-hex commit, or
/// `None` when it cannot.
pub type ResolveCommit<'a> = &'a dyn Fn(&str) -> Option<String>;

/// Write `ota-manifest.json`, `core.z` and `engine.z` into `ota_dir` for the
/// split package in `package_dir` (its `manifest.json` and merged image),
/// whose `core.bin`/`engine.bin` are already in `ota_dir`.
///
/// Returns `None`, having written nothing and warned once, for a **dev**
/// build whose commit is unknown; the same build at a **release** version
/// is an error. Any previous run's OTA files are removed first, so a
/// refused build never leaves a stale manifest beside fresh parts.
pub fn write_ota_files(
    def: &BuildDef,
    package_dir: &Path,
    ota_dir: &Path,
    resolve_commit: ResolveCommit<'_>,
) -> Result<Option<OtaFiles>> {
    remove_stale_ota_files(ota_dir)?;

    let read = |dir: &Path, name: &str| -> Result<Vec<u8>> {
        let path = dir.join(name);
        std::fs::read(&path).with_context(|| format!("reading {}", path.display()))
    };
    let package_bytes = read(package_dir, PACKAGE_MANIFEST_FILE)?;
    let package = DistributionManifest::parse(&package_bytes)?;
    let Some(split) = &package.split else {
        bail!(
            "{} has no `split` block: only a split package has OTA files",
            package_dir.join(PACKAGE_MANIFEST_FILE).display()
        );
    };
    let [image_entry] = package.images.as_slice() else {
        bail!(
            "a split package flashes one merged image, this one lists {}",
            package.images.len()
        );
    };
    let image = read(package_dir, &image_entry.path)?;
    ensure!(
        image.len() as u64 == image_entry.size_bytes && sha256_hex(&image) == image_entry.sha256,
        "{} is not the image the package manifest describes",
        image_entry.path
    );

    // Identity, from the manifest core the image carries.
    let identity = CoreIdentity::read(&package)?;
    ensure!(
        identity.target == def.id,
        "the image's manifest core says target `{}`, but it was packaged as `{}`",
        identity.target,
        def.id
    );
    ensure!(
        identity.chip == def.chip.name,
        "the image's manifest core says chip `{}`, but build def `{}` is `{}`",
        identity.chip,
        def.id,
        def.chip.name
    );
    let Some(commit) = full_commit(&identity.commit, resolve_commit) else {
        let why = format!(
            "the build's commit `{}` does not resolve to a full commit in this checkout",
            identity.commit
        );
        if is_release_version(&identity.version) {
            bail!(
                "release {} cannot be packaged for OTA: {why}",
                identity.version
            );
        }
        eprintln!(
            "warning: no OTA files for {} (dev version {}): {why}",
            def.id, identity.version
        );
        return Ok(None);
    };
    let build_id = format!("{}+{}", identity.version, &commit[..12]);
    ensure!(
        split.build_id == build_id,
        "the split block's build id `{}` is not version+commit[..12] `{build_id}`",
        split.build_id
    );

    let requires = requires_of(split, &image)?;

    // The pieces: the files beside the package, which must be the image's
    // bytes where the split block places them.
    let core = read(ota_dir, CORE_BIN)?;
    let engine = read(ota_dir, ENGINE_BIN)?;
    for (name, piece, bytes) in [
        (CORE_BIN, &split.core, &core),
        (ENGINE_BIN, &split.engine, &engine),
    ] {
        ensure!(
            piece.slice(&image)? == bytes.as_slice() && sha256_hex(bytes) == piece.sha256,
            "{name} is not the image's bytes at {}, where the split block places it",
            piece.offset
        );
    }
    // Packager assertion (never a published field): the core's digest slot
    // holds SHA-256 of `engine.bin` exactly as flashed.
    check_core_carries_engine_digest(&core, &engine)?;

    let core_z = pack_piece(PieceKind::Core, &core);
    let engine_z = pack_piece(PieceKind::Engine, &engine);

    let manifest = OtaManifest {
        format: OTA_MANIFEST_FORMAT,
        target: identity.target,
        chip: identity.chip,
        version: identity.version,
        commit,
        wire_proto: identity.wire_proto,
        requires,
        core: piece_file(CORE_BIN, &core),
        engine: piece_file(ENGINE_BIN, &engine),
        encodings: vec![EncodingEntry::deflate1(Encoding1 {
            id: u32::from(ENCODING_1),
            codec: ENCODING_1_CODEC.to_string(),
            chunk_bytes: CHUNK,
            window_bytes: WINDOW,
            core: encoded_file(CORE_Z, &core_z),
            engine: encoded_file(ENGINE_Z, &engine_z),
        })],
        package: PackageRef {
            file: PACKAGE_JSON.to_string(),
            length: package_bytes.len() as u64,
            sha256: sha256_hex(&package_bytes),
            image: piece_file(&image_entry.path, &image),
        },
    };
    manifest
        .validate()
        .map_err(|e| anyhow::anyhow!("the OTA manifest written for {}: {e}", def.id))?;
    ensure!(
        manifest.build_id() == split.build_id,
        "the OTA manifest's build id `{}` is not the split block's `{}`",
        manifest.build_id(),
        split.build_id
    );

    // The streams first, the manifest last: a manifest on disk always has
    // its files.
    let write = |name: &str, bytes: &[u8]| -> Result<()> {
        let path = ota_dir.join(name);
        std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))
    };
    write(CORE_Z, &core_z.stream)?;
    write(ENGINE_Z, &engine_z.stream)?;
    write(OTA_MANIFEST_FILE, &manifest.to_json_bytes())?;
    Ok(Some(OtaFiles {
        manifest,
        core: core_z,
        engine: engine_z,
    }))
}

impl OtaFiles {
    /// The package step's report: piece sizes, `.z` sizes and ratios, chunk
    /// counts, and how many chunks go raw.
    pub fn summary(&self) -> String {
        let line = |name: &str, raw: u64, z: &EncodedPiece| {
            let raw_chunks = z.chunks.iter().filter(|c| **c == 0).count();
            format!(
                "  {name}: {raw} B -> .z {} B ({:.1} % of raw; sent with raw chunks {:.1} %), \
                 {} chunks, {raw_chunks} raw",
                z.stream.len(),
                100.0 * z.stream.len() as f64 / raw as f64,
                100.0 * z.ratio(raw as usize),
                z.chunks.len(),
            )
        };
        let m = &self.manifest;
        let total_raw = m.core.length + m.engine.length;
        let total_z = (self.core.stream.len() + self.engine.stream.len()) as u64;
        format!(
            "OTA files for {} {} (build {}):\n{}\n{}\n  total: {total_raw} B -> {total_z} B ({:.1} %)",
            m.target,
            m.version,
            m.build_id(),
            line(CORE_BIN, m.core.length, &self.core),
            line(ENGINE_BIN, m.engine.length, &self.engine),
            100.0 * total_z as f64 / total_raw as f64,
        )
    }
}

/// The identity fields of a package's manifest core.
pub struct CoreIdentity {
    pub target: String,
    pub chip: String,
    pub version: String,
    /// As the image carries it: the first 12 digits, or `unknown`.
    pub commit: String,
    pub wire_proto: u32,
}

impl CoreIdentity {
    pub fn read(package: &DistributionManifest) -> Result<Self> {
        let field = |key: &str| -> Result<String> {
            package
                .core_str(key)
                .map(str::to_string)
                .with_context(|| format!("the package's manifest core has no string `{key}`"))
        };
        let chip = package
            .core
            .get("platform")
            .and_then(|p| p.get("chip"))
            .and_then(serde_json::Value::as_str)
            .context("the package's manifest core has no `platform.chip`")?
            .to_string();
        let wire_proto = package
            .core
            .get("wireProto")
            .and_then(serde_json::Value::as_u64)
            .and_then(|w| u32::try_from(w).ok())
            .context("the package's manifest core has no `wireProto`")?;
        Ok(Self {
            target: field("target")?,
            chip,
            version: field("version")?,
            commit: field("commit")?,
            wire_proto,
        })
    }
}

/// `requires`, from the split block and the loader's own version word:
/// `layout` is the block's layout integer (which must be the code table's
/// layout 1), `loader` the word's version (which must be the code table's
/// loader 1, the split image's loader). Nothing else.
pub fn requires_of(split: &SplitBlock, image: &[u8]) -> Result<Requires> {
    ensure!(
        split.layout == u32::from(LAYOUT_1),
        "the split block's layout is {}, but lpc_update's code table knows only layout {LAYOUT_1}",
        split.layout
    );
    let in_image = lp_bootctl::find_loader_version(split.loader.slice(image)?);
    let declared = split
        .loader
        .version
        .context("the split block's loader carries no `version`")?;
    ensure!(
        declared == in_image,
        "the split block says loader version {declared}, the loader's own word says {in_image}"
    );
    ensure!(
        declared == LOADER_1 && lp_bootctl::LOADER_VERSION == LOADER_1,
        "the loader is version {declared} (lp-bootctl's is {}), but lpc_update's code table \
         names the split image's loader {LOADER_1}",
        lp_bootctl::LOADER_VERSION
    );
    Ok(Requires {
        layout: LAYOUT_1,
        loader: declared,
    })
}

/// The full commit for a manifest core's short one, checked to extend it.
fn full_commit(short: &str, resolve: ResolveCommit<'_>) -> Option<String> {
    if short.len() < 7 || !short.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let full = resolve(short)?;
    (is_lower_hex(&full, COMMIT_HEX_LEN) && full.starts_with(short)).then_some(full)
}

/// `git rev-parse` in `repo_root`: the commit a short sha names there.
pub fn git_resolve_commit(repo_root: &Path, short: &str) -> Option<String> {
    let output = std::process::Command::new("git")
        .current_dir(repo_root)
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{short}^{{commit}}"),
        ])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn remove_stale_ota_files(ota_dir: &Path) -> Result<()> {
    for name in [OTA_MANIFEST_FILE, CORE_Z, ENGINE_Z] {
        let path = ota_dir.join(name);
        if path.exists() {
            std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
        }
    }
    Ok(())
}

fn piece_file(name: &str, bytes: &[u8]) -> PieceFile {
    PieceFile {
        file: name.to_string(),
        length: bytes.len() as u64,
        sha256: sha256_hex(bytes),
    }
}

fn encoded_file(name: &str, z: &EncodedPiece) -> EncodedPieceFile {
    EncodedPieceFile {
        file: name.to_string(),
        length: z.stream.len() as u64,
        sha256: sha256_hex(&z.stream),
        chunks: z.chunks.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::ota_fixture::{COMMIT, Fixture};
    use super::*;

    #[test]
    fn writes_a_manifest_the_schema_and_the_crate_accept() {
        let fx = Fixture::new("2026.10.05-3");
        let written = fx.write().unwrap().expect("a release with a commit writes");
        let bytes = std::fs::read(fx.ota_dir().join(OTA_MANIFEST_FILE)).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        super::super::release_check::schema_errors(&value).unwrap();
        let m = OtaManifest::parse_valid(&bytes).unwrap();
        assert_eq!(m, written.manifest);
        assert_eq!(
            m.requires,
            Requires {
                layout: 1,
                loader: 1
            }
        );
        assert!(value.get("buildId").is_none(), "the build id is derived");
        assert_eq!(m.commit, COMMIT);
        assert_eq!(m.build_id(), format!("2026.10.05-3+{}", &COMMIT[..12]));
        assert_eq!(
            (m.target.as_str(), m.chip.as_str()),
            ("esp32c6-4mb", "esp32c6")
        );
        assert_eq!(m.wire_proto, 36);
        assert_eq!(m.package.file, PACKAGE_JSON);

        // The .z files and the index agree, and the encoding is the rule's.
        let e = m.encoding1().unwrap();
        assert_eq!((e.chunk_bytes, e.window_bytes), (4096, 32768));
        for (z, name, raw) in [
            (&e.core, CORE_Z, &fx.core),
            (&e.engine, ENGINE_Z, &fx.engine),
        ] {
            let stream = std::fs::read(fx.ota_dir().join(name)).unwrap();
            m.verify(name, &stream).unwrap();
            assert_eq!(z.chunks.len(), raw.len().div_ceil(4096));
            assert!(z.chunks.contains(&0), "{name}: random chunks go raw");
            assert!(z.chunks.iter().any(|c| *c > 0), "{name}: repeats compress");
        }
        assert!(written.summary().contains("raw"));
    }

    #[test]
    fn rerunning_gives_byte_identical_files() {
        let fx = Fixture::new("2026.10.05-3");
        let names = [OTA_MANIFEST_FILE, CORE_Z, ENGINE_Z];
        fx.write().unwrap().unwrap();
        let first: Vec<Vec<u8>> = names
            .iter()
            .map(|n| std::fs::read(fx.ota_dir().join(n)).unwrap())
            .collect();
        fx.write().unwrap().unwrap();
        for (name, before) in names.iter().zip(first) {
            assert_eq!(
                std::fs::read(fx.ota_dir().join(name)).unwrap(),
                before,
                "{name}"
            );
        }
    }

    #[test]
    fn a_target_string_that_is_not_the_build_def_is_refused() {
        let fx = Fixture::new("2026.10.05-3");
        fx.edit_package(|m| m["core"]["target"] = "esp32c6-8mb".into());
        let error = fx.write().unwrap_err().to_string();
        assert!(error.contains("target `esp32c6-8mb`"), "{error}");
        assert!(!fx.ota_dir().join(OTA_MANIFEST_FILE).exists());
    }

    #[test]
    fn an_unknown_commit_refuses_a_release_and_skips_a_dev_build() {
        let fx = Fixture::new("2026.10.05-3");
        fx.edit_package(|m| m["core"]["commit"] = "unknown".into());
        let error = fx.write().unwrap_err().to_string();
        assert!(error.contains("release 2026.10.05-3"), "{error}");
        assert!(error.contains("`unknown`"), "{error}");

        let dev = Fixture::new("abc1234-dirty-101500PT");
        // A previous run's files are cleared even when nothing is written.
        dev.write().unwrap().unwrap();
        dev.edit_package(|m| m["core"]["commit"] = "unknown".into());
        assert!(dev.write().unwrap().is_none());
        for name in [OTA_MANIFEST_FILE, CORE_Z, ENGINE_Z] {
            assert!(!dev.ota_dir().join(name).exists(), "{name}");
        }
    }

    #[test]
    fn a_build_id_that_is_not_version_plus_commit_is_refused() {
        let fx = Fixture::new("2026.10.05-3");
        fx.edit_package(|m| m["split"]["buildId"] = "2026.10.05-3+000000000000".into());
        let error = fx.write().unwrap_err().to_string();
        assert!(
            error.contains("build id `2026.10.05-3+000000000000`"),
            "{error}"
        );
    }

    #[test]
    fn a_core_whose_digest_slot_is_not_the_engines_is_refused() {
        let fx = Fixture::new("2026.10.05-3");
        fx.corrupt_digest_slot();
        let error = fx.write().unwrap_err().to_string();
        assert!(error.contains("engine digest slot"), "{error}");
    }

    #[test]
    fn a_part_that_is_not_the_images_bytes_is_refused() {
        let fx = Fixture::new("2026.10.05-3");
        let mut engine = fx.engine.clone();
        engine[100] ^= 1;
        std::fs::write(fx.ota_dir().join(ENGINE_BIN), engine).unwrap();
        let error = fx.write().unwrap_err().to_string();
        assert!(
            error.contains("engine.bin is not the image's bytes"),
            "{error}"
        );
    }

    #[test]
    fn requires_is_the_code_tables_layout_and_loader() {
        let fx = Fixture::new("2026.10.05-3");
        fx.edit_package(|m| m["split"]["layout"] = 2.into());
        let error = fx.write().unwrap_err().to_string();
        assert!(error.contains("layout is 2"), "{error}");

        let fx = Fixture::new("2026.10.05-3");
        fx.edit_package(|m| m["split"]["loader"]["version"] = 2.into());
        let error = fx.write().unwrap_err().to_string();
        assert!(error.contains("loader version 2"), "{error}");
    }

    #[test]
    fn full_commit_must_extend_the_short_one() {
        let ok = |_: &str| Some(COMMIT.to_string());
        assert_eq!(full_commit(&COMMIT[..12], &ok).as_deref(), Some(COMMIT));
        assert_eq!(full_commit("unknown", &ok), None);
        assert_eq!(full_commit("0123456789ab", &ok), None);
        let short = |_: &str| Some(COMMIT[..20].to_string());
        assert_eq!(full_commit(&COMMIT[..12], &short), None);
    }
}
