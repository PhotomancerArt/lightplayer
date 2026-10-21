//! Every relocation's target in an Xtensa link, read from the **linked
//! bytes** — the S3's counterpart of
//! [`crate::section_graph::read_relocations`].
//!
//! Three things make `S + A` the wrong answer on the S3's linker (GNU ld,
//! read off its output; never its source):
//!
//! 1. **Stale addends.** `--emit-relocs` keeps a zero addend on relocations
//!    against a merged section's symbol: in the spike's pass 2, 18,355 of
//!    30,236 `R_XTENSA_32` read `<section> + 0` while the word they patch
//!    points well inside it. Their `S + A` aimed half the graph's data edges
//!    at the first section of `.rodata`. The word itself is the target.
//! 2. **Literal coalescing.** Equal literals of different functions share
//!    one pool slot, so a function's `l32r` may load from another
//!    function's pool. An edge to that pool would make the other function's
//!    code reachable through it (its pool is tied to it); the spike's first
//!    run made the whole engine core that way, through `lp_engine_entry`'s
//!    pool. So an `l32r`'s edge is to **the address the slot holds** — when
//!    the slot carries an `R_XTENSA_32`; a plain constant is no edge.
//! 3. **Narrowed calls.** Relaxation turns an in-range `l32r` + `callx8`
//!    into `call8`; its target is decoded from the instruction.
//!
//! The decodes are the Xtensa ISA's: `L32R` loads from
//! `((pc + 3) & ~3) + (sign-extended imm16 << 2)` (the offset is always
//! negative: the 16 bits are ones-extended), `CALLn` jumps to
//! `(pc & ~3) + (sign-extended offset18 << 2) + 4`.

use std::collections::HashMap;

use anyhow::{Context, Result};
use object::Endianness;
use object::read::elf::{FileHeader, Rela, SectionHeader, Sym};

use crate::section_graph::Relocations;

/// `R_XTENSA_NONE`, and the label differences `R_XTENSA_DIFF8/16/32`: they
/// name no target.
pub const SKIP_RELOCS: &[u32] = &[0, 17, 18, 19];
/// `R_XTENSA_32`: an absolute word.
const R_XTENSA_32: u32 = 1;
/// `R_XTENSA_SLOT0_OP`: the instruction's operand (`l32r`, `callN`, `j`,
/// branches).
const R_XTENSA_SLOT0_OP: u32 = 20;

/// What one relocation says, before it is an edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// An edge to this address.
    At(u64),
    /// No edge (an `l32r` of a constant).
    None,
}

/// The literal address an `l32r` at `pc` loads, or `None` when `insn` (its
/// three bytes, little-endian) is not an `l32r`.
pub fn l32r_literal(pc: u32, insn: u32) -> Option<u32> {
    if insn & 0xF != 1 {
        return None;
    }
    let imm16 = (insn >> 8) & 0xFFFF;
    let offset = ((imm16 | 0xFFFF_0000) << 2) as i32;
    Some(((pc + 3) & !3).wrapping_add_signed(offset))
}

/// The target of a `callN` at `pc`, or `None` when `insn` is not one.
pub fn call_target(pc: u32, insn: u32) -> Option<u32> {
    if insn & 0xF != 5 {
        return None;
    }
    let offset18 = ((insn >> 6) as i32) << 14 >> 14;
    Some((pc & !3).wrapping_add_signed(offset18 << 2) + 4)
}

/// What a relocation of `r_type` at `at` targets, given the word or the
/// instruction there, every absolute word's value by address, and `S + A`
/// as the fallback for the rest.
pub fn resolve(
    r_type: u32,
    at: u32,
    word: Option<u32>,
    insn: Option<u32>,
    words: &HashMap<u32, u32>,
    s_plus_a: i64,
) -> Target {
    match r_type {
        R_XTENSA_32 => word.map_or(Target::At(s_plus_a as u64), |w| Target::At(u64::from(w))),
        R_XTENSA_SLOT0_OP => match insn {
            Some(i) if l32r_literal(at, i).is_some() => {
                let literal = l32r_literal(at, i).unwrap_or_default();
                words
                    .get(&literal)
                    .map_or(Target::None, |w| Target::At(u64::from(*w)))
            }
            Some(i) if call_target(at, i).is_some() => {
                Target::At(u64::from(call_target(at, i).unwrap_or_default()))
            }
            _ => Target::At(s_plus_a as u64),
        },
        _ => Target::At(s_plus_a as u64),
    }
}

/// Every relocation's target in an Xtensa ELF32 linked with
/// `--emit-relocs`, as `(r_offset, target)`.
pub fn read_xtensa_targets(data: &[u8]) -> Result<Relocations> {
    let header = object::elf::FileHeader32::<Endianness>::parse(data)?;
    let endian = header.endian()?;
    let sections = header.sections(endian, data)?;
    let (symtab_index, _) = sections
        .iter()
        .enumerate()
        .find(|(_, s)| s.sh_type(endian) == object::elf::SHT_SYMTAB)
        .context("no symbol table (was the ELF stripped?)")?;
    let symbols =
        sections.symbol_table_by_index(endian, data, object::SectionIndex(symtab_index))?;

    // The relocated sections' bytes, and every absolute word's value by its
    // address: what an `l32r` that loads that word means.
    let mut patched = Vec::new();
    let mut words = HashMap::new();
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
        let Some((relas, _)) = section.rela(endian, data)? else {
            continue;
        };
        let base = target.sh_addr(endian);
        let bytes = target.data(endian, data)?;
        for rela in relas {
            let at = rela.r_offset(endian);
            if rela.r_type(endian) == R_XTENSA_32
                && let Some(w) = word_at(bytes, base, at)
            {
                words.insert(at, w);
            }
        }
        patched.push((relas, base, bytes));
    }

    let mut list = Vec::new();
    for (relas, base, bytes) in patched {
        for rela in relas {
            let r_type = rela.r_type(endian);
            if SKIP_RELOCS.contains(&r_type) {
                continue;
            }
            let at = rela.r_offset(endian);
            let sym = symbols.symbol(object::SymbolIndex(rela.r_sym(endian) as usize))?;
            let s_plus_a = i64::from(sym.st_value(endian)) + i64::from(rela.r_addend(endian));
            let target = resolve(
                r_type,
                at,
                word_at(bytes, base, at),
                insn_at(bytes, base, at),
                &words,
                s_plus_a,
            );
            if let Target::At(t) = target {
                list.push((u64::from(at), t as i64));
            }
            // A named symbol is a target too (a `sym - k` table base), as
            // the C6's reader has it; a section symbol is its section's
            // start, and a local label no target of its own.
            let kind = sym.st_type();
            if kind != object::elf::STT_SECTION && kind != object::elf::STT_NOTYPE {
                list.push((u64::from(at), i64::from(sym.st_value(endian))));
            }
        }
    }
    Ok(Relocations {
        list,
        entry: u64::from(header.e_entry(endian)),
    })
}

fn word_at(bytes: &[u8], base: u32, at: u32) -> Option<u32> {
    let o = at.checked_sub(base)? as usize;
    Some(u32::from_le_bytes(bytes.get(o..o + 4)?.try_into().ok()?))
}

fn insn_at(bytes: &[u8], base: u32, at: u32) -> Option<u32> {
    let o = at.checked_sub(base)? as usize;
    let b = bytes.get(o..o + 3)?;
    Some(u32::from(b[0]) | u32::from(b[1]) << 8 | u32::from(b[2]) << 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `l32r a8, 0x4205_0050` at `0x4208_6171`, bytes `81 b7 27`
    /// (objdump of the spike's S3 pass-1 ELF prints the word, `27b781`).
    const L32R: u32 = 0x81 | 0xb7 << 8 | 0x27 << 16;
    const L32R_PC: u32 = 0x4208_6171;
    /// `call8 0x4209_22dc` at `0x4205_0095` (`422465`).
    const CALL8: u32 = 0x65 | 0x24 << 8 | 0x42 << 16;

    #[test]
    fn an_l32r_loads_from_behind_it() {
        assert_eq!(l32r_literal(L32R_PC, L32R), Some(0x4205_0050));
        assert_eq!(call_target(L32R_PC, L32R), None);
    }

    #[test]
    fn a_call_decodes_its_target() {
        assert_eq!(call_target(0x4205_0095, CALL8), Some(0x4209_22dc));
        assert_eq!(l32r_literal(0x4205_0095, CALL8), None);
    }

    #[test]
    fn a_stale_zero_addend_is_read_past_through_the_word() {
        // `.engine_rodata + 0` in the relocation; the word points inside it.
        let t = resolve(
            1,
            0x4037_8490,
            Some(0x3D00_6F9C),
            None,
            &HashMap::new(),
            0x3D00_0000,
        );
        assert_eq!(t, Target::At(0x3D00_6F9C));
    }

    #[test]
    fn an_l32r_targets_the_address_its_slot_holds_not_the_pool() {
        let mut words = HashMap::new();
        words.insert(0x4205_0050, 0x4207_cd10); // another function's pool slot
        let t = resolve(20, L32R_PC, None, Some(L32R), &words, 0x4205_0038);
        assert_eq!(t, Target::At(0x4207_cd10), "the function the slot names");
    }

    #[test]
    fn an_l32r_of_a_plain_constant_is_no_edge() {
        let t = resolve(20, L32R_PC, None, Some(L32R), &HashMap::new(), 0x4205_0038);
        assert_eq!(t, Target::None);
    }

    #[test]
    fn a_narrowed_call_is_an_edge_to_its_target() {
        let t = resolve(20, 0x4205_0095, None, Some(CALL8), &HashMap::new(), 0);
        assert_eq!(t, Target::At(0x4209_22dc));
    }
}
