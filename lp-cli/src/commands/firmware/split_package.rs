//! Packaging a split build (`"split": true`): the flashed image, the
//! manifest's `split` block, and the parts beside the package.
//!
//! The flash unit stays **one merged image at `0x0`**: the bootloader, the
//! partition table and `app.bin` (the loader, record sector 0, record
//! sector 1 erased, the core, the engine), up to `app.bin`'s end — so
//! Studio's and the host's flashers write it with no change, and a flasher's
//! erase covers record sector 1, so a stale newer record on a board is gone.
//! It stops at `app.bin`'s end like `espflash save-image --skip-padding`:
//! the tool's whole-chip `merged.bin` (the emulator's) would erase `lpfs`.
//!
//! `core.bin` and `engine.bin` go to `target/firmware-parts/<id>/`, never
//! into the packaged directory: the Studio bundle does not grow by them, and
//! a later Studio slices them out of the merged image by the `split`
//! block's offsets.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lp_fw_split::SplitReport;
use sha2::{Digest, Sha256};

use super::build_def::BuildDef;
use super::distribution_manifest::{SplitBlock, SplitPiece};

/// Where a split def's parts are written, relative to the repo root.
pub const PARTS_ROOT: &str = "target/firmware-parts";

/// What packaging a split build produces.
pub struct SplitPackage {
    /// The flashed image: the merged image up to `app.bin`'s end.
    pub image: Vec<u8>,
    /// The manifest's `split` block.
    pub block: SplitBlock,
    /// The pass-2 ELF, whose manifest core the image must carry.
    pub elf: PathBuf,
    /// Where `core.bin` and `engine.bin` were written.
    pub parts_dir: PathBuf,
}

/// Read a split build's outputs, check them, and lay them out for packaging.
pub fn package_split(repo_root: &Path, def: &BuildDef) -> Result<SplitPackage> {
    let dir = def.split_dir(repo_root);
    let read = |name: &str| -> Result<Vec<u8>> {
        let path = dir.join(name);
        std::fs::read(&path).with_context(|| {
            format!(
                "reading {} — run `lp-cli firmware build {}` first",
                path.display(),
                def.id
            )
        })
    };
    let report: SplitReport = serde_json::from_slice(&read("split.json")?)
        .with_context(|| format!("parsing {}", dir.join("split.json").display()))?;
    let merged = read("merged.bin")?;
    let core = read("core.bin")?;
    let engine = read("engine.bin")?;

    // Each piece is what split.json says, where it says.
    let app_start = lp_bootctl::LOADER_OFFSET as usize;
    let app_end = app_start + report.app_bin.size_bytes as usize;
    if merged.len() < app_end {
        bail!("merged.bin is shorter than app.bin's end ({app_end:#x})");
    }
    for (name, piece, bytes) in [
        ("core.bin", &report.core, core.as_slice()),
        ("engine.bin", &report.engine, engine.as_slice()),
    ] {
        let at = piece.offset as usize;
        if merged.get(at..at + bytes.len()) != Some(bytes) || sha256_hex(bytes) != piece.sha256 {
            bail!("{name} is not the bytes split.json places at {at:#x}");
        }
    }
    check_core_carries_engine_digest(&core, &engine)?;

    let parts_dir = repo_root.join(PARTS_ROOT).join(&def.id);
    std::fs::create_dir_all(&parts_dir)
        .with_context(|| format!("creating {}", parts_dir.display()))?;
    std::fs::write(parts_dir.join("core.bin"), &core)?;
    std::fs::write(parts_dir.join("engine.bin"), &engine)?;

    let hex = |x: u32| format!("{x:#x}");
    let piece = |p: &lp_fw_split::split_build::Piece| SplitPiece {
        offset: hex(p.offset),
        size_bytes: u64::from(p.size_bytes),
        sha256: p.sha256.clone(),
        version: None,
        header: None,
    };
    let block = SplitBlock {
        layout: report.layout,
        page: report.page,
        build_id: report.build_id.clone(),
        loader: SplitPiece {
            version: Some(report.loader_version),
            ..piece(&report.loader)
        },
        core: piece(&report.core),
        engine: SplitPiece {
            header: Some(lp_bootctl::engine_header::ENGINE_HEADER_VERSION),
            ..piece(&report.engine)
        },
    };
    Ok(SplitPackage {
        image: merged[..app_end].to_vec(),
        block,
        elf: dir.join("p2.elf"),
        parts_dir,
    })
}

/// The core's engine digest slot must hold SHA-256 of `engine.bin` exactly
/// as flashed. The split tool patches it and checks; this checks the bytes
/// being shipped, independently, and refuses otherwise. It is never a
/// published field: the engine piece's `sha256` is the one published value.
pub fn check_core_carries_engine_digest(core: &[u8], engine: &[u8]) -> Result<()> {
    use lp_bootctl::engine_digest::{
        ENGINE_DIGEST_LEN, ENGINE_DIGEST_OFFSET, ENGINE_DIGEST_UNPATCHED,
    };
    let prefix = &ENGINE_DIGEST_UNPATCHED[..ENGINE_DIGEST_OFFSET];
    let want: [u8; 32] = Sha256::digest(engine).into();
    let slots: Vec<[u8; 32]> = core
        .windows(ENGINE_DIGEST_LEN)
        .filter(|w| w.starts_with(prefix))
        .filter_map(lp_bootctl::engine_digest::decode)
        .collect();
    // The slot's 8-byte prefix may also appear as a constant the core
    // compares against, so more than one window can decode: the check is
    // that the engine's digest is among them.
    match slots.as_slice() {
        [] => bail!("core.bin carries no engine digest slot"),
        found if found.contains(&want) => Ok(()),
        found => bail!(
            "the core's engine digest slot holds {}, but engine.bin's SHA-256 is {}",
            found
                .iter()
                .map(|d| hex_bytes(d))
                .collect::<Vec<_>>()
                .join(" / "),
            hex_bytes(&want)
        ),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex_bytes(&Sha256::digest(bytes))
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core_with_slot(digest: &[u8; 32]) -> Vec<u8> {
        let mut core = vec![0xE9u8; 100];
        core.extend_from_slice(&lp_bootctl::engine_digest::encode(digest));
        core.extend_from_slice(&[0u8; 50]);
        core
    }

    #[test]
    fn a_core_carrying_the_engines_digest_passes() {
        let engine = b"engine bytes".to_vec();
        let digest: [u8; 32] = Sha256::digest(&engine).into();
        check_core_carries_engine_digest(&core_with_slot(&digest), &engine).unwrap();
    }

    #[test]
    fn a_core_carrying_another_digest_is_refused() {
        let error = check_core_carries_engine_digest(&core_with_slot(&[7; 32]), b"engine")
            .unwrap_err()
            .to_string();
        assert!(error.contains("engine digest slot holds"), "{error}");
        let error = check_core_carries_engine_digest(&[0u8; 200], b"engine")
            .unwrap_err()
            .to_string();
        assert!(error.contains("no engine digest slot"), "{error}");
    }
}
