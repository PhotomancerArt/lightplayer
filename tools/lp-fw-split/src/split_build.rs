//! The whole pipeline: two passes, the verifier, the split, the loader,
//! `app.bin`, `merged.bin`, and `split.json` describing them.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::app_image::{self, PAGE};
use crate::elf_symbol;
use crate::engine_script::{ENGINE_BASE, EngineRules, pass1_script};
use crate::merged_image;
use crate::pass_link;
use crate::reachability::Split;
use crate::section_graph::{SectionGraph, demangled_symbols};
use crate::split_artifacts;
use crate::tree_guard::{TreeState, hex};
use crate::verify_report::Verification;

/// What to build.
pub struct BuildOptions {
    pub repo: PathBuf,
    pub out: PathBuf,
    /// Cargo features for fw-esp32c6 (its defaults are always on).
    pub features: String,
    /// A build def's bootloader override.
    pub bootloader: Option<PathBuf>,
    /// The partition table the image is flashed with.
    pub partitions: PathBuf,
    /// The target (`lp-fw/builds/` id) the image is built as, embedded in
    /// its manifest core; `None` builds it as `unknown`.
    pub target: Option<String>,
}

impl BuildOptions {
    pub fn new(repo: PathBuf, out: PathBuf) -> Self {
        let partitions = repo.join("lp-fw/fw-esp32c6/partitions.csv");
        Self {
            repo,
            out,
            features: "esp32c6,server".into(),
            bootloader: None,
            partitions,
            target: None,
        }
    }
}

/// One piece of the image, for `split.json`.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Piece {
    /// Flash offset.
    pub offset: u32,
    pub size_bytes: u32,
    pub sha256: String,
}

/// The core's copy of the build id (`"<version>+<commit>"`, zero-padded).
pub const BUILD_ID_SYMBOL: &str = "LP_BUILD_ID";
/// The core's engine digest slot.
pub const ENGINE_DIGEST_SYMBOL: &str = "LP_ENGINE_DIGEST";
/// The emulator seam descriptor table (`fw_esp32_common::seam_table!`).
pub const SEAM_TABLE_SYMBOL: &str = "LP_SEAM_TABLE";
/// The layout `lp_bootctl::SplitLayout` describes.
pub const LAYOUT: u32 = 1;

/// `split.json`: where everything is, for the packager and the tests.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SplitReport {
    pub layout: u32,
    pub page: u32,
    pub region_end: u32,
    pub app_version: String,
    /// `"<version>+<commit>"`: the engine header's and the core's.
    pub build_id: String,
    pub loader_version: u16,
    pub loader: Piece,
    pub core: Piece,
    pub engine: Piece,
    pub app_bin: Piece,
    pub merged_sha256: String,
    pub room_left: u32,
    pub pass1_seconds: f64,
    pub pass2_seconds: f64,
    pub core_in_engine: usize,
    pub core_to_engine_edges: usize,
}

/// Run the pipeline into `opts.out`.
pub fn build(opts: &BuildOptions) -> Result<SplitReport> {
    let out = &opts.out;
    fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    let out = out.canonicalize()?;
    let repo = opts.repo.canonicalize()?;

    let before = TreeState::read(&repo)?;
    // One version for both passes: a dirty tree's dev version carries the
    // time, which would otherwise differ between them.
    let app_version = lp_app_version::resolve(&repo.join("lp-fw/fw-esp32c6"));

    let pass1_x = out.join("engine-pass1.x");
    fs::write(&pass1_x, pass1_script())?;
    eprintln!("==> pass 1 ({}; version {app_version})", opts.features);
    let identity = pass_link::Identity {
        features: &opts.features,
        app_version: &app_version,
        target: opts.target.as_deref(),
    };
    let p1 = pass_link::link_firmware(&repo, &out, "p1", &pass1_x, &identity)?;
    let p1_elf = fs::read(&p1.elf)?;
    // The shipped image carries the emulator seam table, and it is a core
    // root: refuse a build that lost it rather than ship an image whose
    // seams no emulator can find.
    if elf_symbol::find_opt(&p1_elf, SEAM_TABLE_SYMBOL)?.is_none() {
        bail!(
            "pass 1 linked no `{SEAM_TABLE_SYMBOL}`: the shipped image must carry the seam table"
        );
    }
    let graph = SectionGraph::load(&p1.elf, &p1.map)?;
    let split = Split::compute(&graph, &core_roots(&graph, &p1_elf)?);
    let rules = EngineRules::from_split(&graph, &split);
    let engine_x = out.join("engine.x");
    fs::write(&engine_x, rules.script())?;
    eprintln!(
        "    engine.x: {} rodata + {} text input sections",
        rules.rodata.len(),
        rules.text.len()
    );

    eprintln!("==> pass 2");
    let p2 = pass_link::link_firmware(&repo, &out, "p2", &engine_x, &identity)?;
    before.check_unchanged(&repo)?;

    let verification = verify(&p2.elf, &p2.map)?;
    fs::write(out.join("verify.txt"), verification.report(25))?;
    eprintln!("{}", verification.verdict_line());
    if !verification.passed() {
        bail!(
            "pass 2 placed core code in the engine region: {}",
            verification.core_in_engine.join(", ")
        );
    }
    let seams = seam_table_placement(&fs::read(&p2.elf)?)?;
    eprintln!("{seams}");

    eprintln!("==> loader");
    let loader_elf_built = pass_link::build_loader(&repo)?;
    let loader_elf = fs::read(&loader_elf_built)?;
    fs::write(out.join("loader.elf"), &loader_elf)?;
    let loader = split_artifacts::app_image(&loader_elf)?;
    fs::write(out.join("loader.bin"), &loader)?;
    let loader_version = lp_bootctl::find_loader_version(&loader);
    if loader_version != lp_bootctl::LOADER_VERSION {
        bail!(
            "loader.bin carries version {loader_version}, not {}",
            lp_bootctl::LOADER_VERSION
        );
    }

    eprintln!("==> split");
    let mut p2_elf = fs::read(&p2.elf)?;
    let (engine_base, mut engine) = split_artifacts::engine_bin(&p2_elf)?;
    if engine_base != ENGINE_BASE {
        bail!("the engine starts at {engine_base:#x}, not {ENGINE_BASE:#x}");
    }
    // The engine header: len and CRC filled, checked against the core's own
    // copy of the build id.
    let build_id = core_build_id(&p2_elf)?;
    lp_bootctl::engine_header::patch(&mut engine, &build_id)
        .map_err(|e| anyhow::anyhow!("engine header: {e:?}"))?;
    let engine_sha256: [u8; 32] = Sha256::digest(&engine).into();
    // The core's digest slot, patched BEFORE its image is made, so the
    // image's checksum and appended hash cover it.
    let slot = elf_symbol::find(&p2_elf, ENGINE_DIGEST_SYMBOL)?;
    lp_bootctl::engine_digest::patch(slot.get_mut(&mut p2_elf), &engine_sha256)
        .map_err(|e| anyhow::anyhow!("engine digest slot: {e:?}"))?;
    if lp_bootctl::engine_digest::decode(slot.get(&p2_elf)) != Some(engine_sha256) {
        bail!("the core's digest slot does not hold engine.bin's SHA-256 after patching");
    }
    let core = split_artifacts::app_image(&split_artifacts::core_elf(&p2_elf)?)?;
    fs::write(out.join("engine.bin"), &engine)?;
    fs::write(out.join("core.bin"), &core)?;

    let (factory_offset, factory_len) = factory_extent(&opts.partitions)?;
    if factory_offset != lp_bootctl::LOADER_OFFSET {
        bail!("`factory` starts at {factory_offset:#x}, not where the loader goes");
    }
    let region_end = factory_offset + factory_len;
    let build_id_text = build_id_text(&build_id);
    let build = lp_bootctl::build_hash(build_id_text.as_bytes());
    let app = app_image::assemble(&loader, &core, &engine, region_end, build)?;
    fs::write(out.join("app.bin"), &app.bytes)?;
    let merged = merged_image::merge(
        &loader_elf,
        &app.bytes,
        &opts.partitions,
        opts.bootloader.as_deref(),
    )?;
    fs::write(out.join("merged.bin"), &merged)?;

    let piece = |offset: u32, bytes: &[u8]| Piece {
        offset,
        size_bytes: bytes.len() as u32,
        sha256: hex(&Sha256::digest(bytes)),
    };
    let report = SplitReport {
        layout: LAYOUT,
        page: PAGE,
        region_end,
        app_version: app_version.clone(),
        build_id: build_id_text,
        loader_version,
        loader: piece(lp_bootctl::LOADER_OFFSET, &loader),
        core: piece(app.core_off, &core),
        engine: piece(app.engine_off, &engine),
        app_bin: piece(lp_bootctl::LOADER_OFFSET, &app.bytes),
        merged_sha256: hex(&Sha256::digest(&merged)),
        room_left: app.room_left,
        pass1_seconds: p1.took.as_secs_f64(),
        pass2_seconds: p2.took.as_secs_f64(),
        core_in_engine: verification.core_in_engine.len(),
        core_to_engine_edges: verification.core_to_engine_edges,
    };
    fs::write(
        out.join("split.json"),
        serde_json::to_string_pretty(&report)?,
    )?;
    eprintln!(
        "loader {} B · core {} B @{:#x} · engine {} B @{:#x} · app.bin {} B · room left {} B ({} KiB)",
        app.loader_len,
        app.core_len,
        app.core_off,
        app.engine_len,
        app.engine_off,
        app.bytes.len(),
        app.room_left,
        app.room_left / 1024
    );
    eprintln!(
        "pass 1 link: {:.0} s, pass 2 link: {:.0} s",
        p1.took.as_secs_f64(),
        p2.took.as_secs_f64()
    );
    Ok(report)
}

/// Verify one link: its graph, its split, the verdict.
pub fn verify(elf: &Path, map: &Path) -> Result<Verification> {
    let graph = SectionGraph::load(elf, map)?;
    let bytes = fs::read(elf)?;
    let split = Split::compute(&graph, &core_roots(&graph, &bytes)?);
    let names = demangled_symbols(&bytes)?;
    Ok(Verification::run(
        &graph,
        &split,
        u64::from(ENGINE_BASE),
        &names,
    ))
}

/// Nodes that are core whatever reaches them: the digest slot and the
/// build id the packager reads and patches by name, and — when the image
/// has one — the emulator seam table, so it and the seam functions it names
/// land in the core whatever the engine does (dev and harness images may
/// lack it; [`build`] refuses a shipped image that does).
pub fn core_roots(graph: &SectionGraph, elf: &[u8]) -> Result<Vec<usize>> {
    let required = [ENGINE_DIGEST_SYMBOL, BUILD_ID_SYMBOL]
        .map(|name| elf_symbol::find(elf, name).map(|s| (name, s.vaddr)));
    let mut named = Vec::new();
    for r in required {
        named.push(r?);
    }
    if let Some(table) = elf_symbol::find_opt(elf, SEAM_TABLE_SYMBOL)? {
        named.push((SEAM_TABLE_SYMBOL, table.vaddr));
    }
    roots_at(graph, &named)
}

/// The input sections holding each named address.
fn roots_at(graph: &SectionGraph, named: &[(&str, u32)]) -> Result<Vec<usize>> {
    named
        .iter()
        .map(|&(name, vaddr)| {
            graph
                .node_at(i64::from(vaddr))
                .with_context(|| format!("`{name}` is in no input section of the map"))
        })
        .collect()
}

/// Where the seam table and every seam function it names were placed: all
/// of it must be in the core (below the engine region), or the build fails.
pub fn seam_table_placement(elf: &[u8]) -> Result<String> {
    let table = elf_symbol::find(elf, SEAM_TABLE_SYMBOL)?;
    let lines = seam_sites_in_core(table.vaddr, table.get(elf))?;
    Ok(lines.join("\n"))
}

/// The table at `vaddr` and its entries, one line each; an error names the
/// first one at or above [`ENGINE_BASE`].
fn seam_sites_in_core(vaddr: u32, bytes: &[u8]) -> Result<Vec<String>> {
    let view = match lp_seam::table::read(bytes, 0) {
        Ok(lp_seam::table::Read::Match(view)) => view,
        Ok(lp_seam::table::Read::Mismatch { abi }) => {
            bail!("`{SEAM_TABLE_SYMBOL}` has abi {abi:016x}, not this tree's")
        }
        Err(e) => bail!("`{SEAM_TABLE_SYMBOL}` is not a seam table: {e:?}"),
    };
    let core = |what: &str, at: u32| -> Result<String> {
        if at >= ENGINE_BASE {
            bail!("{what} @{at:#010x} is in the engine region: the seam table must be a core root");
        }
        Ok(format!("    {what} @{at:#010x} (core)"))
    };
    let mut lines = vec![core(&format!("seam table {SEAM_TABLE_SYMBOL}"), vaddr)?];
    for e in view.entries() {
        let name = lp_seam::SeamDecl::by_id(e.id).map_or("?", |d| d.symbol);
        lines.push(core(name, e.function)?);
        if e.engaged != 0 {
            lines.push(core(&format!("{name} engaged byte"), e.engaged)?);
        }
    }
    Ok(lines)
}

/// The core's copy of the build id.
fn core_build_id(elf: &[u8]) -> Result<[u8; lp_bootctl::engine_header::ENGINE_BUILD_ID_LEN]> {
    let sym = elf_symbol::find(elf, BUILD_ID_SYMBOL)?;
    sym.get(elf)
        .try_into()
        .with_context(|| format!("`{BUILD_ID_SYMBOL}` is {} B, not 64", sym.size))
}

/// A zero-padded build id as text.
fn build_id_text(field: &[u8]) -> String {
    let len = field.iter().position(|b| *b == 0).unwrap_or(field.len());
    String::from_utf8_lossy(&field[..len]).into_owned()
}

/// `factory`'s `(offset, len)` in the table the image is flashed with.
pub fn factory_extent(partitions: &Path) -> Result<(u32, u32)> {
    let table =
        espflash::flasher::parse_partition_table(partitions).map_err(|e| anyhow::anyhow!("{e}"))?;
    let factory = table
        .find("factory")
        .with_context(|| format!("no `factory` partition in {}", partitions.display()))?;
    Ok((factory.offset(), factory.size()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::section_graph::Node;

    #[test]
    fn the_seam_table_is_a_root_beside_the_digest_and_the_build_id() {
        let node = |vma| Node {
            vma,
            size: 0x100,
            out: ".rodata".into(),
            desc: format!("/x/a.o:(.rodata.{vma:x})"),
        };
        let graph = SectionGraph::build(
            vec![node(0x4200_0000), node(0x4200_1000), node(0x4200_2000)],
            &[],
            0x4200_0000,
        );
        let roots = roots_at(
            &graph,
            &[
                (ENGINE_DIGEST_SYMBOL, 0x4200_0010),
                (BUILD_ID_SYMBOL, 0x4200_1010),
                (SEAM_TABLE_SYMBOL, 0x4200_2040),
            ],
        )
        .unwrap();
        assert_eq!(roots, [0, 1, 2]);
        let err = roots_at(&graph, &[(SEAM_TABLE_SYMBOL, 0x4300_0000)]).unwrap_err();
        assert!(err.to_string().contains(SEAM_TABLE_SYMBOL), "{err}");
    }

    #[test]
    fn a_seam_site_in_the_engine_region_fails_the_build() {
        let core_fn = 0x4210_0000u32;
        let ok = seam_sites_in_core(0x4201_0000, &table(core_fn)).unwrap();
        assert_eq!(ok.len(), 2);
        assert!(ok[1].contains("lp_seam_ws281x_wait_step"), "{ok:?}");
        let err = seam_sites_in_core(0x4201_0000, &table(ENGINE_BASE + 0x40)).unwrap_err();
        assert!(err.to_string().contains("engine region"), "{err}");
        let err = seam_sites_in_core(ENGINE_BASE, &table(core_fn)).unwrap_err();
        assert!(err.to_string().contains("seam table"), "{err}");
    }

    /// A one-entry table naming `function` for the LED wait seam.
    fn table(function: u32) -> Vec<u8> {
        use lp_seam::table::*;
        let mut b = vec![0u8; OFFSET_ENTRIES + ENTRY_LEN];
        b[..16].copy_from_slice(&MAGIC);
        b[OFFSET_ABI..OFFSET_ABI + 8].copy_from_slice(&lp_seam::SEAM_ABI_ID.to_le_bytes());
        b[OFFSET_COUNT..OFFSET_COUNT + 4].copy_from_slice(&1u32.to_le_bytes());
        let e = OFFSET_ENTRIES;
        b[e..e + 2].copy_from_slice(&lp_seam::ws281x_wait_step::ID.to_le_bytes());
        b[e + 4..e + 8].copy_from_slice(&function.to_le_bytes());
        b
    }
}
