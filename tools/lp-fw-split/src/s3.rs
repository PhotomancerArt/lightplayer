//! SPIKE (`spike-s3-split-image`): the same split for `fw-esp32s3` (Xtensa
//! LX7, GNU ld), beside the C6 pipeline and sharing its graph, its walk and
//! its verifier. What differs, and why:
//!
//! - **The linker is GNU ld** (`xtensa-esp32s3-elf-gcc -Wl,…`): its map is
//!   not lld's ([`parse_gnu_map`]), and it relaxes — literal coalescing
//!   (two functions' equal literals merged into one slot, the dropped
//!   slot's relocations kept as `R_XTENSA_NONE`) and longcall narrowing.
//! - **Xtensa relocations.** The image carries three kinds:
//!   `R_XTENSA_NONE` (skipped), `R_XTENSA_32` (absolute words: literal pool
//!   entries, tables, vtables) and `R_XTENSA_SLOT0_OP` (the `l32r` that
//!   loads a literal). Every call the compiler emits is `l32r` + `callx8`,
//!   so a call is an edge through its literal, and **no call has a range**.
//! - **The literal pair.** `l32r` reaches only 256 KiB *backwards*, so a
//!   function's `.literal.X` must sit just before its `.text.X` in the same
//!   region. Coalescing can point `.text.X`'s `l32r` at another function's
//!   slot, so the graph ties `.literal.X` and `.text.X` both ways
//!   ([`tie_literal_pairs`]): either one reached makes both core.
//! - **Two views of one engine.** The S3 MMU maps entry `n` behind both
//!   `0x3C00_0000 + n·64 KiB` (data) and `0x4200_0000 + n·64 KiB`
//!   (instructions). The engine's rodata is linked at
//!   [`ENGINE_DROM`](lp_bootctl::split_layout_s3::ENGINE_DROM), its code at
//!   the instruction view of the very next byte, so one run of flash pages
//!   mapped once serves both (esp-hal's `.rotext_dummy`, again).

use std::collections::HashSet;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result, bail};
use espflash::elf::ElfFirmwareImage;
use espflash::flasher::{FlashData, FlashFrequency, FlashMode, FlashSettings, FlashSize};
use espflash::targets::{Chip, XtalFrequency};
use lp_bootctl::split_layout_s3::{
    ENGINE_DROM, ENGINE_IROM, ENGINE_WINDOW, S3_PAGE, S3_REGION_START, S3SplitLayout,
};
use lp_bootctl::{BOOT_RECORD_SECTORS, BootRecord, LOADER_MAX_LEN, LOADER_OFFSET};
use sha2::{Digest, Sha256};

use crate::elf_symbol;
use crate::engine_script::{is_rust_object, object_pattern};
use crate::image_end::pad_image_end;
use crate::reachability::Split;
use crate::section_graph::{Node, Relocations, SectionGraph, demangled_symbols};
use crate::split_artifacts::section_spans;
use crate::verify_report::Verification;

pub const S3_TARGET: &str = "xtensa-esp32s3-none-elf";
pub const S3_PROFILE: &str = "release-esp32s3";
/// `R_XTENSA_NONE` and the `DIFF8/16/32` label differences: no target.
const XTENSA_SKIP_RELOCS: &[u32] = &[0, 17, 18, 19];
/// The bus distance between the two views of one MMU entry.
const IBUS_FROM_DBUS: u32 = 0x4200_0000 - 0x3C00_0000;
/// The S3 flash: today's 8 MB floor.
pub const S3_FLASH_LEN: usize = 8 * 1024 * 1024;

/// What one S3 build needs.
pub struct S3Options {
    pub repo: PathBuf,
    pub out: PathBuf,
    pub features: Option<String>,
    /// The S3 split loader's ELF (`lp-fw/fw-esp32s3-loader`), if built.
    pub loader_elf: Option<PathBuf>,
}

/// One pass's link.
pub struct S3Pass {
    pub elf: PathBuf,
    pub map: PathBuf,
    pub took: Duration,
}

/// The whole spike pipeline: two passes, the verifier, the cut, and — given
/// a loader — `app.bin` and an 8 MiB `merged.bin` behind the IDF bootloader.
pub fn build_s3(opts: &S3Options) -> Result<()> {
    fs::create_dir_all(&opts.out)?;
    let out = opts.out.canonicalize()?;
    let repo = opts.repo.canonicalize()?;
    let app_version = lp_app_version::resolve(&repo.join("lp-fw/fw-esp32s3"));

    let pass1_x = out.join("engine-pass1.x");
    fs::write(&pass1_x, pass1_script())?;
    eprintln!("==> pass 1 (fw-esp32s3; version {app_version})");
    let p1 = link_s3(&repo, &out, "p1", &pass1_x, opts, &app_version)?;
    let (graph, extra) = load_graph(&p1.elf, &p1.map)?;
    eprintln!(
        "    graph: {} nodes, {} relocations ({} unresolved), {} literal pairs tied",
        graph.nodes.len(),
        graph.relocs,
        graph.unresolved,
        extra
    );
    let p1_bytes = fs::read(&p1.elf)?;
    let split = Split::compute(&graph, &core_roots(&graph, &p1_bytes)?);
    let rules = S3EngineRules::from_split(&graph, &split);
    let engine_x = out.join("engine.x");
    fs::write(&engine_x, rules.script())?;
    eprintln!(
        "    engine.x: {} rodata + {} text input sections (+{} literal rules)",
        rules.rodata.len(),
        rules.text.len(),
        rules.literals
    );

    eprintln!("==> pass 2");
    let p2 = link_s3(&repo, &out, "p2", &engine_x, opts, &app_version)?;
    let verification = verify_s3(&p2.elf, &p2.map)?;
    fs::write(out.join("verify.txt"), verification.report(30))?;
    eprintln!("{}", verification.verdict_line());
    if !verification.passed() {
        bail!(
            "pass 2 placed core code in the engine region: {}",
            verification.core_in_engine.join(", ")
        );
    }

    eprintln!("==> split");
    let p2_elf = fs::read(&p2.elf)?;
    let mut engine = engine_bin(&p2_elf)?;
    let build_id_sym = elf_symbol::find(&p2_elf, "LP_BUILD_ID")?;
    let mut build_id = [0u8; lp_bootctl::engine_header::ENGINE_BUILD_ID_LEN];
    build_id.copy_from_slice(build_id_sym.get(&p2_elf));
    lp_bootctl::engine_header::patch(&mut engine, &build_id)
        .map_err(|e| anyhow::anyhow!("engine header: {e:?}"))?;
    let mut core = core_image(&crate::split_artifacts::core_elf(&p2_elf)?)?;
    // espflash's address-0 padding, re-aimed at the core's .bss (the C6's
    // 2026-10-14 defect: a loader copies it to address 0).
    let bss = elf_symbol::value(&p2_elf, "_bss_start")?..elf_symbol::value(&p2_elf, "_bss_end")?;
    for p in crate::core_padding::aim_padding_at_bss(&mut core, bss)? {
        eprintln!(
            "    core.bin: padding segment {} ({} B) aimed at .bss {:#010x}",
            p.index, p.len, p.addr
        );
    }
    fs::write(out.join("engine.bin"), &engine)?;
    fs::write(out.join("core.bin"), &core)?;
    let segs = crate::core_segments::segments(&core)?;
    for s in &segs {
        eprintln!(
            "    core seg {}: {:#010x} +{:#x} (image off {:#x})",
            s.index, s.addr, s.len, s.data_offset
        );
    }
    eprintln!(
        "core.bin {} B · engine.bin {} B (sha256 {})",
        core.len(),
        engine.len(),
        hex(&Sha256::digest(&engine))
    );
    let mut summary = String::new();
    let _ = writeln!(
        summary,
        "pass1 {:.0} s, pass2 {:.0} s\ncore.bin {} B\nengine.bin {} B",
        p1.took.as_secs_f64(),
        p2.took.as_secs_f64(),
        core.len(),
        engine.len()
    );

    let Some(loader_elf) = &opts.loader_elf else {
        fs::write(out.join("summary.txt"), summary)?;
        eprintln!("(no --loader-elf: stopping before app.bin)");
        return Ok(());
    };
    eprintln!("==> app.bin + merged.bin");
    let loader_elf = fs::read(loader_elf)?;
    let loader = loader_image(&loader_elf)?;
    fs::write(out.join("loader.bin"), &loader)?;
    let layout = S3SplitLayout::from_factory(LOADER_OFFSET, 0x60_0000).unwrap();
    let build = lp_bootctl::build_hash(build_id_text(&build_id).as_bytes());
    let app = assemble(&loader, &core, &engine, &layout, build)?;
    fs::write(out.join("app.bin"), &app.0)?;
    let merged = merge(
        &loader_elf,
        &app.0,
        &repo.join("lp-fw/fw-esp32s3/partitions.csv"),
    )?;
    fs::write(out.join("merged.bin"), &merged)?;
    let _ = writeln!(
        summary,
        "loader.bin {} B\ncore @{:#x}\nengine @{:#x}\nroom left {} B",
        loader.len(),
        S3_REGION_START,
        app.1,
        layout.region_end.saturating_sub(app.1 + engine.len() as u32)
    );
    fs::write(out.join("summary.txt"), &summary)?;
    eprint!("{summary}");
    Ok(())
}

/// One link of `fw-esp32s3`, in its own crate directory.
pub fn link_s3(
    repo: &Path,
    out: &Path,
    name: &str,
    script: &Path,
    opts: &S3Options,
    app_version: &str,
) -> Result<S3Pass> {
    let crate_dir = repo.join("lp-fw/fw-esp32s3");
    let main_rs = crate_dir.join("src/main.rs");
    fs::File::options()
        .write(true)
        .open(&main_rs)
        .and_then(|f| f.set_modified(SystemTime::now()))?;
    let map = out.join(format!("{name}.map"));
    let started = Instant::now();
    let mut command = Command::new("cargo");
    for key in [
        "RUSTUP_TOOLCHAIN",
        "RUSTC",
        "RUSTDOC",
        "CARGO",
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_MANIFEST_DIR",
    ] {
        command.env_remove(key);
    }
    if let Some(gcc) = xtensa_gcc_dir() {
        let path = std::env::var("PATH").unwrap_or_default();
        command.env("PATH", format!("{}:{path}", gcc.display()));
    }
    command
        .current_dir(&crate_dir)
        .env("LP_SPLIT_LINK", "1")
        .env("APP_VERSION", app_version)
        .args(["rustc", "--quiet", "--profile", S3_PROFILE]);
    if let Some(f) = &opts.features {
        command.args(["--features", f]);
    }
    let output = command
        .args(["--message-format", "json-render-diagnostics"])
        .args(["--", "-C", "link-arg=-Wl,--emit-relocs"])
        .arg("-C")
        .arg(format!("link-arg=-Wl,-Map={}", map.display()))
        .arg("-C")
        .arg(format!("link-arg=-Wl,-T{}", script.display()))
        .stderr(Stdio::inherit())
        .output()?;
    if !output.status.success() {
        bail!("the {name} link of fw-esp32s3 failed");
    }
    let mut built = None;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Ok(msg) = serde_json::from_str::<serde_json::Value>(line)
            && msg["reason"] == "compiler-artifact"
            && msg["target"]["name"] == "fw-esp32s3"
            && let Some(p) = msg["executable"].as_str()
        {
            built = Some(PathBuf::from(p));
        }
    }
    let built = built.context("cargo reported no fw-esp32s3 executable")?;
    let elf = out.join(format!("{name}.elf"));
    fs::copy(&built, &elf)?;
    Ok(S3Pass {
        elf,
        map,
        took: started.elapsed(),
    })
}

/// The espup toolchain's GNU binutils, when not already on `PATH` (the
/// justfile's `_xt-gcc-dir`).
fn xtensa_gcc_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let root = Path::new(&home).join(".rustup/toolchains/esp/xtensa-esp-elf");
    let mut dirs: Vec<_> = fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|e| e.path().join("xtensa-esp-elf/bin"))
        .filter(|p| p.join("xtensa-esp32s3-elf-gcc").exists())
        .collect();
    dirs.sort();
    dirs.pop()
}

/// The graph of one S3 link, its output sections normalised to the names
/// the shared walk knows, and literal pairs tied. The count of ties.
pub fn load_graph(elf: &Path, map: &Path) -> Result<(SectionGraph, usize)> {
    let text = fs::read_to_string(map)?;
    let nodes = parse_gnu_map(&text)
        .into_iter()
        .map(|mut n| {
            n.out = normalise_out(&n.out).to_string();
            n
        })
        .collect();
    let bytes = fs::read(elf)?;
    let relocs = read_xtensa_targets(&bytes)?;
    let mut graph = SectionGraph::build(nodes, &relocs.list, relocs.entry);
    let ties = tie_literal_pairs(&mut graph);
    Ok((graph, ties))
}

/// Every relocation's target, read from the **linked bytes**, never from
/// `S + A`: GNU ld's `--emit-relocs` on Xtensa keeps a stale zero addend on
/// relocations against a merged section's symbol (18,355 of 30,236
/// `R_XTENSA_32` in the spike's pass 2 read `<section> + 0` while the word
/// they patch points well inside it), which aimed half the graph's data
/// edges at the first section of `.rodata`.
///
/// - `R_XTENSA_32`: the 32-bit word at `r_offset` is the target.
/// - `R_XTENSA_SLOT0_OP` on an `l32r` (op0 = 1): the literal it loads,
///   `((pc + 3) & ~3) + (imm16 | 0xFFFF0000) << 2` (Xtensa ISA RM, L32R).
/// - on a `callN` (op0 = 5): `(pc & ~3) + (offset18 << 2) + 4` (CALL0..12).
/// - anything else (branches, `j`): `S + A`, which is within the function.
///
/// A named (non-section) symbol is also a target, as in the C6 reader (a
/// `sym - k` table base).
pub fn read_xtensa_targets(data: &[u8]) -> Result<Relocations> {
    use object::Endianness;
    use object::read::elf::{FileHeader, Rela, SectionHeader, Sym};
    let header = object::elf::FileHeader32::<Endianness>::parse(data)?;
    let endian = header.endian()?;
    let sections = header.sections(endian, data)?;
    let (symtab_index, _) = sections
        .iter()
        .enumerate()
        .find(|(_, s)| s.sh_type(endian) == object::elf::SHT_SYMTAB)
        .context("no symbol table")?;
    let symbols =
        sections.symbol_table_by_index(endian, data, object::SectionIndex(symtab_index))?;
    let mut list = Vec::new();
    let mut words = std::collections::HashMap::new();
    let mut pending = Vec::new();
    for section in sections.iter() {
        if section.sh_type(endian) != object::elf::SHT_RELA {
            continue;
        }
        let target = sections.section(object::SectionIndex(section.sh_info(endian) as usize))?;
        if target.sh_flags(endian) & object::elf::SHF_ALLOC == 0
            || target.sh_type(endian) == object::elf::SHT_NOBITS
        {
            continue;
        }
        let base = target.sh_addr(endian);
        let bytes = target.data(endian, data)?;
        let word = |at: u32| -> Option<u32> {
            let o = at.checked_sub(base)? as usize;
            Some(u32::from_le_bytes(bytes.get(o..o + 4)?.try_into().ok()?))
        };
        let Some((relas, _)) = section.rela(endian, data)? else {
            continue;
        };
        // First, every absolute word's target, by its address: what an
        // `l32r` that loads that word means.
        for rela in relas {
            if rela.r_type(endian) == 1
                && let Some(w) = word(rela.r_offset(endian))
            {
                words.insert(rela.r_offset(endian), w);
            }
        }
        pending.push((section.sh_info(endian) as usize, relas, base, bytes));
    }
    for (_, relas, base, bytes) in pending {
        let insn = |at: u32| -> Option<u32> {
            let o = at.checked_sub(base)? as usize;
            let b = bytes.get(o..o + 3)?;
            Some(u32::from(b[0]) | u32::from(b[1]) << 8 | u32::from(b[2]) << 16)
        };
        let word = |at: u32| -> Option<u32> {
            let o = at.checked_sub(base)? as usize;
            Some(u32::from_le_bytes(bytes.get(o..o + 4)?.try_into().ok()?))
        };
        for rela in relas {
            let r_type = rela.r_type(endian);
            if XTENSA_SKIP_RELOCS.contains(&r_type) {
                continue;
            }
            let at = rela.r_offset(endian);
            let sym = symbols.symbol(object::SymbolIndex(rela.r_sym(endian) as usize))?;
            let s_plus_a = i64::from(sym.st_value(endian)) + i64::from(rela.r_addend(endian));
            let resolved = match r_type {
                1 => word(at).map(i64::from),
                20 => match insn(at).map(|i| (i, i & 0xF)) {
                    // An `l32r`: what it loads, when that word is an address
                    // (it carries an `R_XTENSA_32`); a plain constant is no
                    // edge. Never the pool itself: coalescing shares one
                    // function's pool slot with another's `l32r`.
                    Some((i, 1)) => {
                        let imm16 = (i >> 8) & 0xFFFF;
                        let off = ((imm16 | 0xFFFF_0000) << 2) as i32;
                        let lit = ((at + 3) & !3).wrapping_add_signed(off);
                        match words.get(&lit) {
                            Some(w) => Some(i64::from(*w)),
                            None => continue,
                        }
                    }
                    Some((i, 5)) => {
                        let off18 = ((i >> 6) as i32) << 14 >> 14;
                        Some(i64::from((at & !3).wrapping_add_signed(off18 << 2) + 4))
                    }
                    _ => None,
                },

                _ => None,
            };
            if let Ok(dbg) = std::env::var("LP_SPLIT_DEBUG_FROM")
                && let Some((lo, hi)) = dbg.split_once(',')
                && let (Ok(lo), Ok(hi)) = (u32::from_str_radix(lo, 16), u32::from_str_radix(hi, 16))
                && (lo..hi).contains(&at)
            {
                eprintln!(
                    "edge {at:#x} type {r_type} resolved {:?} s+a {s_plus_a:#x} sym_type {}",
                    resolved.map(|r| format!("{r:#x}")),
                    sym.st_type()
                );
            }
            list.push((u64::from(at), resolved.unwrap_or(s_plus_a)));
            if sym.st_type() != object::elf::STT_SECTION && sym.st_type() != object::elf::STT_NOTYPE
            {
                list.push((u64::from(at), i64::from(sym.st_value(endian))));
            }
        }
    }
    Ok(Relocations {
        list,
        entry: u64::from(header.e_entry(endian)),
    })
}

/// SPIKE debugging: the chain of input sections by which the core's roots
/// reach the first node whose description contains `needle`.
pub fn why_s3(elf: &Path, map: &Path, needle: &str) -> Result<Vec<String>> {
    let (graph, _) = load_graph(elf, map)?;
    let bytes = fs::read(elf)?;
    let names = demangled_symbols(&bytes)?;
    let mut roots: Vec<usize> = graph
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.is_ram_code() || n.out == ".flash.appdesc")
        .map(|(i, _)| i)
        .collect();
    if let Some(e) = graph.node_at(graph.entry as i64) {
        roots.push(e);
    }
    roots.extend(core_roots(&graph, &bytes)?);
    let mut parent = std::collections::HashMap::new();
    let mut queue: std::collections::VecDeque<usize> = roots.iter().copied().collect();
    for r in &roots {
        parent.insert(*r, usize::MAX);
    }
    let label = |i: usize| {
        let n = &graph.nodes[i];
        let sym = names
            .range(n.vma..n.vma + n.size.max(1))
            .next()
            .and_then(|(_, v)| v.first())
            .map(|(_, s)| s.chars().take(140).collect::<String>())
            .unwrap_or_default();
        format!("{:#010x} {} {} [{}]", n.vma, n.out, n.object_and_section().1.chars().take(80).collect::<String>(), sym)
    };
    while let Some(i) = queue.pop_front() {
        if graph.nodes[i].desc.contains(needle) && parent[&i] != usize::MAX {
            let mut chain = vec![label(i)];
            let mut at = parent[&i];
            while at != usize::MAX {
                chain.push(label(at));
                at = parent[&at];
            }
            chain.reverse();
            return Ok(chain);
        }
        if let Some(ts) = graph.edges.get(&i) {
            for t in ts {
                if !parent.contains_key(t) {
                    parent.insert(*t, i);
                    queue.push_back(*t);
                }
            }
        }
    }
    Ok(vec!["(not reached)".into()])
}

/// The S3's output sections, by the name the shared graph gives their kind:
/// `.vectors` is RAM code (the bootloader loads it), `.rodata.wifi` flash.
fn normalise_out(out: &str) -> &str {
    match out {
        ".vectors" => ".rwtext",
        ".rodata.wifi" => ".rodata",
        o if o.starts_with(".rtc_slow") => ".rtc_fast.data",
        o => o,
    }
}

/// An edge from `.text.X` to its own `.literal.X` (and `.text` to
/// `.literal`) of one object, so a core function's pool counts as core.
/// Placement does the rest: the engine script writes every engine
/// function's pool right before it (`l32r` reaches only 256 KiB back).
pub fn tie_literal_pairs(graph: &mut SectionGraph) -> usize {
    let mut by_key = std::collections::HashMap::new();
    for (i, n) in graph.nodes.iter().enumerate() {
        let (obj, sec) = n.object_and_section();
        if let Some(rest) = sec.strip_prefix(".text") {
            by_key.insert((obj.to_string(), rest.to_string()), i);
        }
    }
    let mut ties = 0;
    for i in 0..graph.nodes.len() {
        let (obj, sec) = graph.nodes[i].object_and_section();
        let Some(rest) = sec.strip_prefix(".literal") else {
            continue;
        };
        if let Some(&t) = by_key.get(&(obj.to_string(), rest.to_string())) {
            // One way: a core function's pool is core. Never back: a pool
            // coalescing gave other functions' `l32r`s is not their code.
            graph.edges.entry(t).or_default().insert(i);
            ties += 1;
        }
    }
    ties
}

/// The input sections of a GNU ld map (`-Map`), as the shared graph's
/// nodes (`object:(section)` descriptions, lld's spelling).
///
/// Read from the map's own shape (GNU ld's documentation of `-Map`, and the
/// file in front of us): after `Linker script and memory map`, an output
/// section starts at column 0; an input section is one space then its name,
/// followed — on the same line, or on the next when the name is long — by
/// its address, its size and its object. Symbols (address and name only),
/// `*fill*` and `(size before relaxing)` lines are not sections.
pub fn parse_gnu_map(text: &str) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut out = String::new();
    let mut pending: Option<String> = None;
    let mut started = false;
    for line in text.lines() {
        if !started {
            started = line.starts_with("Linker script and memory map");
            continue;
        }
        if line.is_empty() {
            continue;
        }
        let first = line.as_bytes()[0];
        if first != b' ' {
            out = line.split_whitespace().next().unwrap_or("").to_string();
            pending = None;
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let toks: Vec<&str> = line.split_whitespace().collect();
        if indent == 1 && !toks[0].starts_with('*') {
            let name = toks[0].to_string();
            if let Some(n) = node_from(&out, &name, &toks[1..]) {
                nodes.push(n);
                pending = None;
            } else {
                pending = Some(name);
            }
            continue;
        }
        if let Some(name) = pending.take()
            && let Some(n) = node_from(&out, &name, &toks)
        {
            nodes.push(n);
        }
    }
    nodes
}

fn node_from(out: &str, name: &str, toks: &[&str]) -> Option<Node> {
    if toks.len() < 3 {
        return None;
    }
    let hex = |t: &str| u64::from_str_radix(t.strip_prefix("0x")?, 16).ok();
    let vma = hex(toks[0])?;
    let size = hex(toks[1])?;
    Some(Node {
        vma,
        size,
        out: out.to_string(),
        desc: format!("{}:({name})", toks[2..].join(" ")),
    })
}

/// The core's named roots: the build id the packager reads.
pub fn core_roots(graph: &SectionGraph, elf: &[u8]) -> Result<Vec<usize>> {
    let id = elf_symbol::find(elf, "LP_BUILD_ID")?;
    Ok(vec![
        graph
            .node_at(i64::from(id.vaddr))
            .context("LP_BUILD_ID is in no input section")?,
    ])
}

/// The S3 pass-1 script: only the header placed, at the engine's data view.
pub fn pass1_script() -> String {
    format!(
        "MEMORY {{\n  ENGINE_RO : ORIGIN = {ENGINE_DROM:#x}, LENGTH = {ENGINE_WINDOW:#x}\n}}\n\
         SECTIONS {{\n  .engine_rodata : ALIGN(4) {{\n    KEEP(*(.engine_header))\n  }} > ENGINE_RO\n}}\n\
         INSERT BEFORE .rodata;\n"
    )
}

/// The engine's placement rules, literal pools beside their code.
pub struct S3EngineRules {
    pub rodata: Vec<String>,
    pub text: Vec<String>,
    pub literals: usize,
}

impl S3EngineRules {
    pub fn from_split(graph: &SectionGraph, split: &Split) -> Self {
        let mut engine: Vec<usize> = split.engine(graph);
        engine.sort_by_key(|i| graph.nodes[*i].vma);
        let mut rules = Self {
            rodata: Vec::new(),
            text: Vec::new(),
            literals: 0,
        };
        let mut seen = HashSet::new();
        for i in engine {
            let n = &graph.nodes[i];
            if !n.is_flash() {
                continue;
            }
            let (obj, sec) = n.object_and_section();
            if !is_rust_object(obj) || sec == ".engine_header" {
                continue;
            }
            let pat = object_pattern(obj);
            if n.out == ".text" {
                // Each function's pool right before it, whatever pass 1's
                // relaxation left of it (a fully coalesced pool is no node,
                // but the pass-2 link starts from the object's own).
                if let Some(rest) = sec.strip_prefix(".text") {
                    let lit = format!("*{pat}(.literal{rest})");
                    if seen.insert(lit.clone()) {
                        rules.text.push(lit);
                        rules.literals += 1;
                    }
                    let txt = format!("*{pat}({sec})");
                    if seen.insert(txt.clone()) {
                        rules.text.push(txt);
                    }
                }
                // A `.literal.X` node itself is emitted with its `.text.X`.
            } else {
                rules.rodata.push(format!("*{pat}({sec})"));
            }
        }
        rules
    }

    /// The whole `engine.x`: the header and rodata at the data view, the
    /// code at the instruction view of the next byte.
    pub fn script(&self) -> String {
        let mut s = String::new();
        s.push_str("/* SPIKE split-link image (fw-esp32s3): generated by tools/lp-fw-split. */\n");
        let _ = writeln!(
            s,
            "MEMORY {{\n  ENGINE_RO : ORIGIN = {ENGINE_DROM:#x}, LENGTH = {ENGINE_WINDOW:#x}\n  ENGINE_TX : ORIGIN = {ENGINE_IROM:#x}, LENGTH = {ENGINE_WINDOW:#x}\n}}"
        );
        s.push_str("SECTIONS {\n  .engine_rodata : ALIGN(4) {\n    KEEP(*(.engine_header))\n");
        for r in &self.rodata {
            let _ = writeln!(s, "    {r}");
        }
        let _ = writeln!(
            s,
            "    . = ALIGN(4);\n  }} > ENGINE_RO\n  .engine_text ALIGN(ADDR(.engine_rodata) + SIZEOF(.engine_rodata) + {IBUS_FROM_DBUS:#x}, 4) : {{"
        );
        for r in &self.text {
            let _ = writeln!(s, "    {r}");
        }
        s.push_str("  } > ENGINE_TX\n}\nINSERT BEFORE .rodata;\n");
        s
    }
}

/// Verify an S3 link: the shared verifier, with the instruction view folded
/// onto the data view so one engine base (`ENGINE_DROM`) splits both.
pub fn verify_s3(elf: &Path, map: &Path) -> Result<Verification> {
    let (mut graph, _) = load_graph(elf, map)?;
    let bytes = fs::read(elf)?;
    let split = Split::compute(&graph, &core_roots(&graph, &bytes)?);
    let names = demangled_symbols(&bytes)?;
    for n in &mut graph.nodes {
        if (0x4200_0000..0x4400_0000).contains(&n.vma) {
            n.vma -= u64::from(IBUS_FROM_DBUS);
        }
    }
    Ok(Verification::run(
        &graph,
        &split,
        u64::from(ENGINE_DROM),
        &names,
    ))
}

/// `engine.bin`: `.engine_rodata` from the data view, `.engine_text` at its
/// instruction-view address folded onto the data view, zero between.
pub fn engine_bin(elf: &[u8]) -> Result<Vec<u8>> {
    let spans = section_spans(elf, &[".engine_rodata", ".engine_text"])?;
    if spans.len() != 2 {
        bail!("the ELF has no engine sections");
    }
    let lin = |a: u32| {
        if a >= 0x4200_0000 {
            a - IBUS_FROM_DBUS
        } else {
            a
        }
    };
    let base = spans.iter().map(|s| lin(s.addr)).min().unwrap();
    if base != ENGINE_DROM {
        bail!("the engine starts at {base:#x}, not {ENGINE_DROM:#x}");
    }
    let end = spans.iter().map(|s| lin(s.addr) + s.size).max().unwrap();
    let mut out = vec![0u8; (end - base) as usize];
    for s in &spans {
        let src = &elf[s.file_offset as usize..(s.file_offset + s.size) as usize];
        let at = (lin(s.addr) - base) as usize;
        out[at..at + src.len()].copy_from_slice(src);
    }
    Ok(out)
}

fn flash_data(partitions: Option<&Path>) -> Result<FlashData> {
    FlashData::new(
        None,
        partitions,
        None,
        None,
        FlashSettings::new(
            Some(FlashMode::Dio),
            Some(FlashSize::_8Mb),
            Some(FlashFrequency::_40Mhz),
        ),
        0,
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}

/// The ESP application image espflash makes of `elf` for the S3.
pub fn core_image(elf: &[u8]) -> Result<Vec<u8>> {
    let image = ElfFirmwareImage::try_from(elf).map_err(|e| anyhow::anyhow!("{e}"))?;
    let built = Chip::Esp32s3
        .into_target()
        .get_flash_image(&image, flash_data(None)?, None, XtalFrequency::_40Mhz)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let parts: Vec<_> = built.ota_segments().collect();
    match parts.as_slice() {
        [single] => Ok(single.data.to_vec()),
        _ => bail!("espflash made {} app segments, expected one", parts.len()),
    }
}

/// The loader's image: the same builder.
pub fn loader_image(elf: &[u8]) -> Result<Vec<u8>> {
    core_image(elf)
}

/// `app.bin` for layout 2: loader, record 0 (seq 1, proven), record 1
/// erased, the core at the region's start, the engine at the next page.
/// The bytes and the engine's offset.
pub fn assemble(
    loader: &[u8],
    core: &[u8],
    engine: &[u8],
    layout: &S3SplitLayout,
    build: u32,
) -> Result<(Vec<u8>, u32)> {
    if loader.len() as u32 > LOADER_MAX_LEN {
        bail!("loader is {} B, over {LOADER_MAX_LEN}", loader.len());
    }
    let core_len = core.len() as u32;
    // `LP_S3_CORE_HIGH=1`: the core at the region's high end and the engine
    // at its start — where every other update leaves them.
    let high = std::env::var_os("LP_S3_CORE_HIGH").is_some();
    let (core_off, engine_off) = if high {
        let at = layout
            .next_core_offset(S3_REGION_START, engine.len() as u32, core_len)
            .context("no room for a high core")?;
        (at, S3_REGION_START)
    } else {
        (
            S3_REGION_START,
            (S3_REGION_START + core_len).div_ceil(S3_PAGE) * S3_PAGE,
        )
    };
    if engine_off + engine.len() as u32 > layout.region_end
        || (high && engine_off + engine.len() as u32 > core_off)
    {
        bail!("core + engine do not fit the region");
    }
    let end = (engine_off + engine.len() as u32).max(core_off + core_len);
    let mut bytes = vec![0xffu8; (end - LOADER_OFFSET) as usize];
    let mut put = |at: u32, data: &[u8]| {
        let at = (at - LOADER_OFFSET) as usize;
        bytes[at..at + data.len()].copy_from_slice(data);
    };
    put(LOADER_OFFSET, loader);
    let record = BootRecord {
        seq: 1,
        core_off,
        core_len,
        build,
        trial: false,
    };
    put(BOOT_RECORD_SECTORS[0], &record.encode());
    put(core_off, core);
    put(engine_off, engine);
    pad_image_end(&mut bytes, LOADER_OFFSET);
    Ok((bytes, engine_off))
}

/// The whole 8 MiB chip: espflash's bundled S3 IDF bootloader at `0x0`,
/// the partition table at `0x8000`, `app.bin` at `factory`.
pub fn merge(loader_elf: &[u8], app: &[u8], partitions: &Path) -> Result<Vec<u8>> {
    let image = ElfFirmwareImage::try_from(loader_elf).map_err(|e| anyhow::anyhow!("{e}"))?;
    let built = Chip::Esp32s3
        .into_target()
        .get_flash_image(
            &image,
            flash_data(Some(partitions))?,
            None,
            XtalFrequency::_40Mhz,
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut out = Vec::with_capacity(S3_FLASH_LEN);
    for segment in built.flash_segments() {
        let addr = segment.addr as usize;
        out.resize(addr.max(out.len()), 0xff);
        out.truncate(addr);
        out.extend_from_slice(&segment.data);
    }
    out.resize(S3_FLASH_LEN, 0xff);
    let at = LOADER_OFFSET as usize;
    out[at..at + app.len()].copy_from_slice(app);
    Ok(out)
}

fn build_id_text(field: &[u8]) -> String {
    let end = field.iter().position(|b| *b == 0).unwrap_or(field.len());
    String::from_utf8_lossy(&field[..end]).into_owned()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gnu_map_gives_input_sections_on_one_line_or_two() {
        let map = "\
Discarded input sections
 .text.gone     0x00000000        0x4 /x/a.o
Linker script and memory map

.text           0x42050020   0x1ff619
 *(.literal .text .literal.* .text.*)
 .literal.f     0x42050020       0x28 /x/a.rcgu.o
 .literal._RNvlong
                0x42050048        0x4 /x/a.rcgu.o
                                0x21c (size before relaxing)
 *fill*         0x4205004c        0x4
 .text.f        0x42050050       0x10 /x/lib.a(m.o)
                0x42050050                f
";
        let nodes = parse_gnu_map(map);
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].desc, "/x/a.rcgu.o:(.literal.f)");
        assert_eq!(nodes[1].vma, 0x4205_0048);
        assert_eq!(nodes[1].size, 4);
        assert_eq!(
            nodes[2].object_and_section(),
            ("/x/lib.a(m.o)", ".text.f")
        );
        assert!(nodes.iter().all(|n| n.out == ".text"));
    }

    #[test]
    fn a_literal_pool_and_its_code_are_tied() {
        let node = |vma, sec: &str| Node {
            vma,
            size: 0x10,
            out: ".text".into(),
            desc: format!("/x/a.rcgu.o:({sec})"),
        };
        let mut g = SectionGraph::build(
            vec![node(0x100, ".literal.f"), node(0x200, ".text.f")],
            &[],
            0,
        );
        assert_eq!(tie_literal_pairs(&mut g), 1);
        assert!(g.edges[&1].contains(&0) && !g.edges.contains_key(&0));
    }
}
