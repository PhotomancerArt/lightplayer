//! Firmware build definitions: the checked-in description of how to produce
//! one shippable firmware variant (`lp-fw/builds/<id>.json`).
//!
//! A build def is a build *input*. It never restates what a build contains —
//! features, wire proto and limits are extracted from the artifact's embedded
//! manifest core (`docs/adr/2026-08-01-firmware-manifest-architecture.md`).
//! See `lp-fw/builds/README.md` for the authoring rules.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// Only accepted `format`. Repo-internal build input, alpha posture:
/// version + refuse, no dual decode.
pub const BUILD_DEF_FORMAT: u32 = 1;

/// Directory holding the checked-in build defs, relative to the repo root.
pub const BUILDS_DIR: &str = "lp-fw/builds";

/// `.json` files in [`BUILDS_DIR`] that are not build defs. `served.json` is
/// a statement *about* build defs — which of them the Studio site ships — so
/// it shares their directory and must never be parsed as one.
const NON_BUILD_DEF_FILES: &[&str] = &["served.json"];

/// One firmware variant.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuildDef {
    /// Schema version of this file; must equal [`BUILD_DEF_FORMAT`].
    pub format: u32,
    /// Variant id; also the packaged output directory name.
    pub id: String,
    /// Human label carried into the distribution manifest.
    pub display_name: String,
    /// Cargo package; the crate directory is `lp-fw/<package>`.
    pub package: String,
    /// Rust target triple.
    pub cargo_target: String,
    /// Cargo profile.
    pub profile: String,
    /// Cargo features **added to** the crate defaults (not `LpFeature`s).
    pub cargo_features: Vec<String>,
    /// Flash size the image header declares; must match `partitions_csv`.
    pub flash_size_mb: u32,
    /// Repo-relative partition table — the file espflash flashes with.
    pub partitions_csv: String,
    /// espflash chip identity.
    pub chip: BuildChip,
}

/// Chip identity block of a build def.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuildChip {
    /// Chip family (`esp32`).
    pub family: String,
    /// Concrete chip, as espflash `--chip` spells it (`esp32c6`).
    pub name: String,
}

impl BuildDef {
    /// espflash's `--flash-size` spelling (`4mb`).
    pub fn flash_size_arg(&self) -> String {
        format!("{}mb", self.flash_size_mb)
    }

    /// Declared flash size in bytes.
    pub fn flash_size_bytes(&self) -> u64 {
        u64::from(self.flash_size_mb) * 1024 * 1024
    }

    /// Crate directory. Linked firmware builds must run here so the crate's
    /// `.cargo/config.toml` and `rust-toolchain.toml` apply (AGENTS.md,
    /// ADR 2026-07-29-per-chip-fw-toolchains).
    pub fn crate_dir(&self, repo_root: &Path) -> Result<PathBuf> {
        let dir = repo_root.join("lp-fw").join(&self.package);
        if !dir.join("Cargo.toml").exists() {
            bail!(
                "build def `{}` names package `{}`, but {} has no Cargo.toml",
                self.id,
                self.package,
                dir.display()
            );
        }
        Ok(dir)
    }

    /// Path of the linked ELF this def's build produces.
    pub fn elf_path(&self, repo_root: &Path) -> PathBuf {
        repo_root
            .join("target")
            .join(&self.cargo_target)
            .join(&self.profile)
            .join(&self.package)
    }
}

/// Parse a build def from JSON, refusing unknown `format`s.
pub fn parse_build_def(json: &str) -> Result<BuildDef> {
    let def: BuildDef = serde_json::from_str(json).context("build def is not valid JSON")?;
    if def.format != BUILD_DEF_FORMAT {
        bail!(
            "build def `{}` has format {} — this lp-cli only understands format {} \
             (build defs are version + refuse; regenerate or update the tool)",
            def.id,
            def.format,
            BUILD_DEF_FORMAT
        );
    }
    if def.cargo_features.is_empty() {
        bail!("build def `{}` lists no cargo features", def.id);
    }
    Ok(def)
}

/// Load every build def under `lp-fw/builds/`, sorted by id.
pub fn load_build_defs(repo_root: &Path) -> Result<Vec<BuildDef>> {
    let dir = repo_root.join(BUILDS_DIR);
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .with_context(|| format!("reading build defs from {}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter(|path| {
            !path
                .file_name()
                .is_some_and(|name| NON_BUILD_DEF_FILES.iter().any(|skip| name == *skip))
        })
        .collect();
    paths.sort();

    let mut defs = Vec::with_capacity(paths.len());
    for path in paths {
        let json = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let def = parse_build_def(&json).with_context(|| format!("parsing {}", path.display()))?;
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        if def.id != stem {
            bail!(
                "build def {} declares id `{}` — the id must match the file name",
                path.display(),
                def.id
            );
        }
        defs.push(def);
    }
    Ok(defs)
}

/// Load one build def by id.
pub fn load_build_def(repo_root: &Path, id: &str) -> Result<BuildDef> {
    let defs = load_build_defs(repo_root)?;
    let known: Vec<&str> = defs.iter().map(|def| def.id.as_str()).collect();
    defs.iter()
        .find(|def| def.id == id)
        .cloned()
        .with_context(|| {
            format!(
                "unknown firmware build `{id}` (known: {})",
                known.join(", ")
            )
        })
}

/// Repository root, found by walking up from the current directory (same
/// heuristic as the hardware manifest store and schema generator).
pub fn find_repo_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    for candidate in cwd.ancestors() {
        if candidate.join("Cargo.toml").exists() && candidate.join(BUILDS_DIR).exists() {
            return Ok(candidate.to_path_buf());
        }
    }
    bail!("could not find repository root from {}", cwd.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    const C6: &str = r#"{
        "format": 1,
        "id": "esp32c6-4mb",
        "displayName": "LightPlayer ESP32-C6 server firmware",
        "package": "fw-esp32c6",
        "cargoTarget": "riscv32imac-unknown-none-elf",
        "profile": "release-esp32",
        "cargoFeatures": ["esp32c6", "server"],
        "flashSizeMb": 4,
        "partitionsCsv": "lp-fw/fw-esp32c6/partitions.csv",
        "chip": { "family": "esp32", "name": "esp32c6" }
    }"#;

    #[test]
    fn parses_a_build_def() {
        let def = parse_build_def(C6).unwrap();
        assert_eq!(def.id, "esp32c6-4mb");
        assert_eq!(def.package, "fw-esp32c6");
        assert_eq!(def.cargo_features, ["esp32c6", "server"]);
        assert_eq!(def.flash_size_arg(), "4mb");
        assert_eq!(def.flash_size_bytes(), 4 * 1024 * 1024);
        assert_eq!(def.chip.name, "esp32c6");
        assert_eq!(
            def.elf_path(Path::new("/repo")),
            Path::new("/repo/target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6")
        );
    }

    /// Version + refuse: a future format is an error, never a partial decode.
    #[test]
    fn refuses_an_unknown_format() {
        let json = C6.replace("\"format\": 1", "\"format\": 2");
        let error = parse_build_def(&json).unwrap_err().to_string();
        assert!(error.contains("format 2"), "{error}");
    }

    #[test]
    fn refuses_unknown_fields_and_missing_fields() {
        let extra = C6.replace("\"format\": 1", "\"format\": 1, \"lpFeatures\": []");
        assert!(parse_build_def(&extra).is_err());
        let missing = C6.replace("\"flashSizeMb\": 4,", "");
        assert!(parse_build_def(&missing).is_err());
    }

    /// The flash size lives in three places that cannot read each other: the
    /// build def (canonical), a justfile var, and the crate's
    /// `.cargo/config.toml` runner. Mismatched values boot-loop a board with
    /// "exceeds flash chip size" (ADR 2026-07-30-esp32s3-partition-floor), so
    /// the mirrors are pinned here rather than left to a comment.
    ///
    /// Covers every chip. It used to check only the S3, and the C6 was the one
    /// that paid for that: its runner carried neither `--flash-size` NOR
    /// `--partition-table` until 2026-08-02, so `cargo run` flashed a
    /// DEFAULT partition table over the real one — dropping `lpfs` (user
    /// content) and `bootctl` silently, since the app still boots from
    /// 0x10000. A desk jig was found in exactly that state.
    #[test]
    fn flash_size_mirrors_agree_with_the_defs() {
        let repo_root = find_repo_root().unwrap();
        let justfile = std::fs::read_to_string(repo_root.join("justfile")).unwrap();

        for (id, just_var) in [
            ("esp32s3-8mb", "s3_flash_size"),
            ("esp32v3-4mb", "v3_flash_size"),
            ("esp32c6-4mb", "c6_flash_size"),
        ] {
            let def = load_build_def(&repo_root, id).unwrap();
            assert!(
                justfile.contains(&format!("{just_var} := \"{}\"", def.flash_size_arg())),
                "justfile {just_var} drifted from lp-fw/builds/{id}.json"
            );

            let config = std::fs::read_to_string(
                def.crate_dir(&repo_root)
                    .unwrap()
                    .join(".cargo/config.toml"),
            )
            .unwrap();
            // Match the `runner = ...` LINE, not the file. These configs carry
            // long comments that name the very flags being asserted, so a
            // whole-file `contains` passes on the prose while the runner is
            // wrong — verified by mutation: deleting `--partition-table` from
            // the runner did not fail this test until it was narrowed here.
            let runner = config
                .lines()
                .map(str::trim_start)
                .find(|line| line.starts_with("runner") && line.contains("espflash flash"))
                .unwrap_or_else(|| {
                    panic!("{}: no espflash runner in .cargo/config.toml", def.package)
                });
            assert!(
                runner.contains(&format!("--flash-size {}", def.flash_size_arg())),
                "{} runner has no/wrong --flash-size vs lp-fw/builds/{id}.json:\n  {runner}",
                def.package
            );
            // The partition table is the other half, and the one whose absence
            // is silent: a wrong --flash-size boot-loops loudly, a missing
            // --partition-table just quietly erases lpfs and bootctl.
            assert!(
                runner.contains("--partition-table"),
                "{} runner passes no --partition-table; espflash will substitute a \
                 default table and drop lpfs/bootctl:\n  {runner}",
                def.package
            );
        }
    }

    /// The checked-in defs parse, and their ids match their file names.
    #[test]
    fn checked_in_defs_are_valid() {
        let repo_root = find_repo_root().unwrap();
        let defs = load_build_defs(&repo_root).unwrap();
        assert!(
            defs.iter().any(|def| def.id == "esp32c6-4mb"),
            "esp32c6-4mb build def missing"
        );
        for def in &defs {
            assert!(
                repo_root.join(&def.partitions_csv).exists(),
                "{} points at a missing partition table",
                def.id
            );
            def.crate_dir(&repo_root).unwrap();
        }
    }
}
