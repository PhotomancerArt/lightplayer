//! The whole pipeline: two passes, the verifier, the split, the loader,
//! `app.bin`, `merged.bin`, and `split.json` describing them.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;
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
        }
    }
}

/// One piece of the image, for `split.json`.
#[derive(Serialize, Clone, Debug)]
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
/// The layout `lp_bootctl::SplitLayout` describes.
pub const LAYOUT: u32 = 1;

/// `split.json`: where everything is, for the packager and the tests.
#[derive(Serialize, Clone, Debug)]
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
    let p1 = pass_link::link_firmware(&repo, &out, "p1", &pass1_x, &opts.features, &app_version)?;
    let graph = SectionGraph::load(&p1.elf, &p1.map)?;
    let split = Split::compute(&graph, &core_roots(&graph, &fs::read(&p1.elf)?)?);
    let rules = EngineRules::from_split(&graph, &split);
    let engine_x = out.join("engine.x");
    fs::write(&engine_x, rules.script())?;
    eprintln!(
        "    engine.x: {} rodata + {} text input sections",
        rules.rodata.len(),
        rules.text.len()
    );

    eprintln!("==> pass 2");
    let p2 = pass_link::link_firmware(&repo, &out, "p2", &engine_x, &opts.features, &app_version)?;
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

    let region_end = factory_end(&opts.partitions)?;
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
/// build id the packager reads and patches by name.
pub fn core_roots(graph: &SectionGraph, elf: &[u8]) -> Result<Vec<usize>> {
    let mut roots = Vec::new();
    for name in [ENGINE_DIGEST_SYMBOL, BUILD_ID_SYMBOL] {
        let sym = elf_symbol::find(elf, name)?;
        let node = graph
            .node_at(i64::from(sym.vaddr))
            .with_context(|| format!("`{name}` is in no input section of the map"))?;
        roots.push(node);
    }
    Ok(roots)
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

/// Where `factory` ends in the table the image is flashed with.
pub fn factory_end(partitions: &Path) -> Result<u32> {
    let table =
        espflash::flasher::parse_partition_table(partitions).map_err(|e| anyhow::anyhow!("{e}"))?;
    let factory = table
        .find("factory")
        .with_context(|| format!("no `factory` partition in {}", partitions.display()))?;
    Ok(factory.offset() + factory.size())
}
