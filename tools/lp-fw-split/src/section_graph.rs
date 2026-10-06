//! The link as a graph: nodes are the linker's **input sections** (the unit a
//! linker script can place), read from lld's `-Map` file; edges are the
//! relocations `--emit-relocs` kept in the ELF, from the input section that
//! holds `r_offset` to the one that holds `S + A`.
//!
//! Written from the ELF32 specification (section headers, `SHT_SYMTAB`,
//! `SHT_RELA`) and lld's map format: four columns (`VMA LMA Size Align`),
//! then the output section at indent 0, its input sections at indent 8 as
//! `object:(section)`, and symbols deeper.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use object::Endianness;
use object::read::elf::{FileHeader, Rela, SectionHeader, Sym};

/// Flash-mapped output sections (the core's and the engine's).
pub const FLASH_OUTS: &[&str] = &[
    ".text",
    ".rodata",
    ".text_gap",
    ".flash.appdesc",
    ".engine_text",
    ".engine_rodata",
];
/// Code the second-stage bootloader loads into RAM: always core.
pub const RAM_CODE_OUTS: &[&str] = &[".trap", ".rwtext", ".rwtext.wifi"];
/// RAM data: nodes, so a relocation into them resolves, but never placed.
pub const RAM_DATA_OUTS: &[&str] = &[
    ".data",
    ".data.wifi",
    ".bss",
    ".noinit",
    ".rtc_fast.text",
    ".rtc_fast.data",
    ".rtc_fast.bss",
    ".rtc_fast.persistent",
    ".stack",
    ".dram2_uninit",
];

/// Relocation types that name no target: `R_RISCV_NONE`, the paired
/// `PCREL_LO12_*` (their symbol is the `auipc` label), the label-difference
/// `ADD*`/`SUB*`/`SET*`/`32_PCREL` family, `ALIGN` and `RELAX`.
const SKIP_RELOCS: &[u32] = &[
    0, 24, 25, 33, 34, 35, 36, 37, 38, 39, 40, 43, 51, 52, 53, 54, 55, 56, 57,
];

/// One input section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub vma: u64,
    pub size: u64,
    /// The output section it was placed in.
    pub out: String,
    /// lld's `object:(section)` text.
    pub desc: String,
}

impl Node {
    /// `(object, section)` from `object:(section)`.
    pub fn object_and_section(&self) -> (&str, &str) {
        match self.desc.split_once(":(") {
            Some((obj, sec)) => (obj, sec.strip_suffix(')').unwrap_or(sec)),
            None => (self.desc.as_str(), ""),
        }
    }

    pub fn is_flash(&self) -> bool {
        FLASH_OUTS.contains(&self.out.as_str()) && self.out != ".text_gap"
    }

    pub fn is_ram_code(&self) -> bool {
        RAM_CODE_OUTS.contains(&self.out.as_str())
    }
}

/// Input sections, sorted by address, and the relocation edges between them.
pub struct SectionGraph {
    pub nodes: Vec<Node>,
    pub edges: HashMap<usize, HashSet<usize>>,
    /// Relocations kept (after the skipped types).
    pub relocs: usize,
    /// Relocations whose source or target is in no node.
    pub unresolved: usize,
    /// The ELF's entry point.
    pub entry: u64,
    starts: Vec<u64>,
}

impl SectionGraph {
    /// Read the map and the ELF of one link.
    pub fn load(elf_path: &Path, map_path: &Path) -> Result<Self> {
        let map =
            std::fs::read(map_path).with_context(|| format!("reading {}", map_path.display()))?;
        let elf =
            std::fs::read(elf_path).with_context(|| format!("reading {}", elf_path.display()))?;
        let nodes = parse_map(&String::from_utf8_lossy(&map));
        let relocs = read_relocations(&elf)?;
        Ok(Self::build(nodes, &relocs.list, relocs.entry))
    }

    /// The graph over `nodes` (any order) with relocations `(r_offset, S + A)`.
    pub fn build(nodes: Vec<Node>, relocs: &[(u64, i64)], entry: u64) -> Self {
        let mut nodes: Vec<Node> = nodes
            .into_iter()
            .filter(|n| {
                n.size > 0
                    && (FLASH_OUTS.contains(&n.out.as_str())
                        || RAM_CODE_OUTS.contains(&n.out.as_str())
                        || RAM_DATA_OUTS.contains(&n.out.as_str()))
            })
            .collect();
        // Stable: two nodes at one address keep the map's order.
        nodes.sort_by_key(|n| n.vma);
        let starts = nodes.iter().map(|n| n.vma).collect();
        let mut graph = Self {
            nodes,
            edges: HashMap::new(),
            relocs: relocs.len(),
            unresolved: 0,
            entry,
            starts,
        };
        for &(r_off, target) in relocs {
            let s = graph.node_at(r_off as i64);
            let t = graph.node_at(target);
            match (s, t) {
                (Some(s), Some(t)) => {
                    if s != t {
                        graph.edges.entry(s).or_default().insert(t);
                    }
                }
                _ => graph.unresolved += 1,
            }
        }
        graph
    }

    /// The node holding `addr`: the last one starting at or before it, if
    /// it reaches that far.
    pub fn node_at(&self, addr: i64) -> Option<usize> {
        if addr < 0 {
            return None;
        }
        let addr = addr as u64;
        let i = self.starts.partition_point(|&s| s <= addr);
        if i == 0 {
            return None;
        }
        let n = &self.nodes[i - 1];
        (n.vma <= addr && addr < n.vma + n.size).then_some(i - 1)
    }
}

/// Every input section in an lld map, as `(vma, size, out, desc)`.
pub fn parse_map(text: &str) -> Vec<Node> {
    let mut out = String::new();
    let mut nodes = Vec::new();
    for line in text.lines() {
        let Some((vma, size, depth, rest)) = parse_map_line(line) else {
            continue;
        };
        if depth == 0 {
            out = rest.trim().to_string();
        } else if rest.contains(":(") && depth <= 8 {
            nodes.push(Node {
                vma,
                size,
                out: out.clone(),
                desc: rest.trim().to_string(),
            });
        }
    }
    nodes
}

/// One map line: `VMA LMA Size Align`, one separating blank, then the
/// indent that says what the line is (0 output section, 8 input section).
fn parse_map_line(line: &str) -> Option<(u64, u64, usize, &str)> {
    let mut rest = line.trim_start();
    let mut cols = [0u64; 4];
    for (k, col) in cols.iter_mut().enumerate() {
        if k > 0 {
            let trimmed = rest.trim_start();
            if trimmed.len() == rest.len() {
                return None;
            }
            rest = trimmed;
        }
        let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
        let tok = &rest[..end];
        let radix_ok = if k == 3 {
            tok.bytes().all(|b| b.is_ascii_digit())
        } else {
            tok.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        };
        if tok.is_empty() || !radix_ok {
            return None;
        }
        *col = u64::from_str_radix(tok, if k == 3 { 10 } else { 16 }).ok()?;
        rest = &rest[end..];
    }
    // Exactly one separator; an align at the end of the line is an empty
    // output-section name.
    let mut chars = rest.chars();
    match chars.next() {
        None => return Some((cols[0], cols[2], 0, "")),
        Some(c) if c.is_whitespace() => {}
        Some(_) => return None,
    }
    let rest = chars.as_str();
    let body = rest.trim_start();
    let depth = rest[..rest.len() - body.len()].chars().count();
    Some((cols[0], cols[2], depth, body))
}

/// The relocations of an ELF32 executable linked with `--emit-relocs`.
pub struct Relocations {
    /// `(r_offset, S + A)` for every kept relocation in an allocated section.
    pub list: Vec<(u64, i64)>,
    pub entry: u64,
}

pub fn read_relocations(data: &[u8]) -> Result<Relocations> {
    let header = object::elf::FileHeader32::<Endianness>::parse(data)?;
    let endian = header.endian()?;
    let sections = header.sections(endian, data)?;
    let Some((symtab_index, _)) = sections
        .iter()
        .enumerate()
        .find(|(_, s)| s.sh_type(endian) == object::elf::SHT_SYMTAB)
    else {
        bail!("no symbol table (was the ELF stripped?)");
    };
    let symbols =
        sections.symbol_table_by_index(endian, data, object::SectionIndex(symtab_index))?;
    let mut list = Vec::new();
    for section in sections.iter() {
        if section.sh_type(endian) != object::elf::SHT_RELA {
            continue;
        }
        let target = sections.section(object::SectionIndex(section.sh_info(endian) as usize))?;
        if target.sh_flags(endian) & object::elf::SHF_ALLOC == 0 {
            continue;
        }
        let Some((relas, _)) = section.rela(endian, data)? else {
            continue;
        };
        for rela in relas {
            let r_type = rela.r_type(endian);
            if SKIP_RELOCS.contains(&r_type) {
                continue;
            }
            let sym = symbols.symbol(object::SymbolIndex(rela.r_sym(endian) as usize))?;
            let at = u64::from(rela.r_offset(endian));
            let value = i64::from(sym.st_value(endian));
            let addend = i64::from(rela.r_addend(endian));
            list.push((at, value + addend));
            // A symbol's own section is a target too, whatever the addend:
            // codegen may address a table as `sym - k` and index from `k`
            // up, and `S + A` then lands in whichever section the link put
            // before it — a different one in each pass. Missing that edge
            // placed a table the core reads into the engine region
            // (docs/defects/2026-10-05-the-split-tool-missed-a-negative-addend-reference.md).
            // A section symbol's value is its section's start, not a target.
            if addend != 0 && sym.st_type() != object::elf::STT_SECTION {
                list.push((at, value));
            }
        }
    }
    Ok(Relocations {
        list,
        entry: u64::from(header.e_entry(endian)),
    })
}

/// Address → `(size, demangled name)` for every defined symbol, for naming
/// nodes in reports.
pub fn demangled_symbols(data: &[u8]) -> Result<BTreeMap<u64, Vec<(u64, String)>>> {
    let header = object::elf::FileHeader32::<Endianness>::parse(data)?;
    let endian = header.endian()?;
    let sections = header.sections(endian, data)?;
    let symbols = sections.symbols(endian, data, object::elf::SHT_SYMTAB)?;
    let mut by_addr: BTreeMap<u64, Vec<(u64, String)>> = BTreeMap::new();
    for sym in symbols.iter() {
        if sym.st_shndx(endian) == object::elf::SHN_UNDEF {
            continue;
        }
        let Ok(name) = sym.name(endian, symbols.strings()) else {
            continue;
        };
        let name = String::from_utf8_lossy(name);
        if name.is_empty() {
            continue;
        }
        let demangled = format!("{:#}", rustc_demangle::demangle(&name));
        by_addr
            .entry(u64::from(sym.st_value(endian)))
            .or_default()
            .push((u64::from(sym.st_size(endian)), demangled));
    }
    Ok(by_addr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_lines_are_read_by_indent() {
        let map = "\
     VMA      LMA     Size Align Out     In      Symbol
       0        0        0     1 PROVIDE( strdup = __esp_radio_strdup )
42000020 42000020     1234     4 .rodata
42000020 42000020       10     4         /x/a.o:(.rodata.a)
42000020 42000020       10     1                 sym_a
42000030 42000030       20     4         /x/lib.rlib(m.o):(.rodata.b)
42030020 42030020      100     4 .text
42030020 42030020       80     2         /x/a.o:(.text.f)
";
        let nodes = parse_map(map);
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].out, ".rodata");
        assert_eq!(nodes[0].desc, "/x/a.o:(.rodata.a)");
        assert_eq!(
            nodes[1].object_and_section(),
            ("/x/lib.rlib(m.o)", ".rodata.b")
        );
        assert_eq!((nodes[2].vma, nodes[2].size), (0x4203_0020, 0x80));
        assert_eq!(nodes[2].out, ".text");
    }

    #[test]
    fn a_relocation_joins_the_sections_holding_its_two_ends() {
        let node = |vma, size, out: &str| Node {
            vma,
            size,
            out: out.into(),
            desc: format!("/x/a.o:(s{vma:x})"),
        };
        let g = SectionGraph::build(
            vec![
                node(0x200, 0x10, ".text"),
                node(0x100, 0x10, ".text"),
                node(0x300, 0, ".text"),
            ],
            &[
                (0x104, 0x208),
                (0x104, 0x104),
                (0x104, 0x999),
                (0x50, 0x100),
            ],
            0x100,
        );
        assert_eq!(g.nodes[0].vma, 0x100, "sorted by address");
        assert_eq!(g.nodes.len(), 2, "empty sections are not nodes");
        assert_eq!(g.edges[&0], HashSet::from([1]));
        assert_eq!(g.unresolved, 2);
        assert_eq!(g.node_at(0x10f), Some(0));
        assert_eq!(g.node_at(0x110), None);
    }

    #[test]
    fn a_negative_addend_reaches_the_symbols_own_section_as_read_gives_it() {
        // `read_relocations` gives a `sym - 0xc` reference as two targets:
        // where `S + A` lands (the section before) and `S` itself.
        let node = |vma, size| Node {
            vma,
            size,
            out: ".rodata".into(),
            desc: format!("/x/a.o:(s{vma:x})"),
        };
        let g = SectionGraph::build(
            vec![node(0x100, 0x10), node(0x200, 0x10), node(0x300, 0x10)],
            &[(0x104, 0x300 - 0xc), (0x104, 0x300)],
            0x100,
        );
        assert!(g.edges[&0].contains(&2), "the table itself is reached");
    }
}
