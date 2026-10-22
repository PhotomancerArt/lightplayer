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
//! 4. **Stale branches.** The radio blobs (GNU-assembled C, linked in since
//!    the S3's radio milestones) keep a relocation on every branch and jump
//!    for the linker's relaxation, against the output section's symbol
//!    with the input section's addend: `.text + 0x5f8` for a `bgeui` inside
//!    `btdm_controller_init`, 2.2 MB further on. `S + A` lands on `.text`'s
//!    first bytes, which are literal pools, and the pool's slots then made
//!    the engine core: a 1.3 MB S3 core with Bluetooth, 2.7 MB with Wi-Fi.
//!    A branch's target is decoded from the instruction, as a call's is.
//!    `R_XTENSA_ASM_EXPAND`/`ASM_SIMPLIFY` mark an `l32r` + `callx` pair
//!    whose `l32r` already carries the edge, and are no edge of their own
//!    (a named symbol on them still is, below). The Rust objects carry
//!    neither: LLVM resolves its branches in the object.
//!
//! The decodes are the Xtensa ISA's: `L32R` loads from
//! `((pc + 3) & ~3) + (sign-extended imm16 << 2)` (the offset is always
//! negative: the 16 bits are ones-extended), `CALLn` jumps to
//! `(pc & ~3) + (sign-extended offset18 << 2) + 4`, and every branch is
//! `pc + 4 + offset` ([`branch_target`]).

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
/// `R_XTENSA_ASM_EXPAND`, `R_XTENSA_ASM_SIMPLIFY`: an `l32r` + `callx`
/// pair the linker may narrow. Its `l32r` carries the edge.
const R_XTENSA_ASM_EXPAND: u32 = 11;
const R_XTENSA_ASM_SIMPLIFY: u32 = 12;
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

/// The target of a branch or jump at `pc`, or `None` when `insn` (its bytes,
/// little-endian) is not one. Every branch adds its offset to `pc + 4`. The
/// formats, by `op0` (bits 3:0), `n` (5:4) and `m` (7:6):
///
/// - `op0 = 6, n = 0`: `J`, an 18-bit signed offset in bits 23:6;
/// - `op0 = 6, n = 1`: `BEQZ/BNEZ/BLTZ/BGEZ` (BRI12), 12 signed bits 23:12;
/// - `op0 = 6, n = 2`: `BEQI/BNEI/BLTI/BGEI` (BRI8), 8 signed bits 23:16;
/// - `op0 = 6, n = 3`: `m = 0` is `ENTRY` (no target); `m = 2, 3` are
///   `BLTUI/BGEUI` (8 signed bits); `m = 1` is chosen by `r` (bits 15:12):
///   `BF`/`BT` (0, 1; 8 signed bits) and `LOOP/LOOPNEZ/LOOPGTZ` (8, 9, 10;
///   8 *unsigned* bits, the loop's end);
/// - `op0 = 7`: the RRI8 branches (`BEQ`, `BNE`, `BLT`, `BBCI`, …), 8 signed
///   bits 23:16;
/// - `op0 = 0xC`, `m = 2, 3`: the narrow `BEQZ.N/BNEZ.N` (16 bits), an
///   unsigned 6-bit offset, bits 5:4 above bits 15:12.
///
/// Checked against objdump's printed targets for every branch relocation in
/// the S3's radio image (4,591 of 4,591).
pub fn branch_target(pc: u32, insn: u32) -> Option<u32> {
    let signed = |bits: u32, width: u32| ((bits << (32 - width)) as i32) >> (32 - width);
    let at = |offset: i32| Some((pc + 4).wrapping_add_signed(offset));
    let (op0, n, m) = (insn & 0xF, (insn >> 4) & 3, (insn >> 6) & 3);
    let imm8 = (insn >> 16) & 0xFF;
    match (op0, n, m) {
        (6, 0, _) => at(signed(insn >> 6, 18)),
        (6, 1, _) => at(signed(insn >> 12, 12)),
        (6, 2, _) | (6, 3, 2 | 3) | (7, _, _) => at(signed(imm8, 8)),
        (6, 3, 1) => match (insn >> 12) & 0xF {
            0 | 1 => at(signed(imm8, 8)),
            8..=10 => at(imm8 as i32),
            _ => None,
        },
        (0xC, _, 2 | 3) => at(((n << 4) | ((insn >> 12) & 0xF)) as i32),
        _ => None,
    }
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
            Some(i) if branch_target(at, i).is_some() => {
                Target::At(u64::from(branch_target(at, i).unwrap_or_default()))
            }
            _ => Target::At(s_plus_a as u64),
        },
        R_XTENSA_ASM_EXPAND | R_XTENSA_ASM_SIMPLIFY => Target::None,
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

    /// Every branch format, each a relocation in the S3's radio image, with
    /// the target objdump prints for it (bytes little-endian).
    #[test]
    fn a_branch_decodes_its_target() {
        let cases: &[(&str, u32, [u8; 3], u32)] = &[
            ("j", 0x4037_d800, [0x06, 0x06, 0x00], 0x4037_d81c),
            ("j back", 0x4228_c209, [0x46, 0xc9, 0xfe], 0x4228_bd32),
            ("bnez", 0x4228_c206, [0x56, 0xc3, 0xb3], 0x4228_bd46),
            ("beqz", 0x4228_cd34, [0x16, 0x06, 0x44], 0x4228_d178),
            ("bnei", 0x4228_b5b5, [0x66, 0x46, 0x27], 0x4228_b5e0),
            ("blti", 0x4228_51fa, [0xa6, 0x12, 0xb6], 0x4228_51b4),
            ("bgeui", 0x4228_51e4, [0xf6, 0x22, 0x04], 0x4228_51ec),
            ("bltui", 0x4037_bd7c, [0xb6, 0x22, 0x11], 0x4037_bd91),
            ("bgeu", 0x4228_51f2, [0xb7, 0xb2, 0x13], 0x4228_5209),
            ("bbsi", 0x4037_d7fd, [0x07, 0xe2, 0xdf], 0x4037_d7e0),
            ("bnez.n", 0x4228_7f45, [0xec, 0x76, 0x21], 0x4228_7f70),
            ("beqz.n", 0x4228_99af, [0x8c, 0x9a, 0x81], 0x4228_99bc),
        ];
        for &(name, pc, b, want) in cases {
            let insn = u32::from(b[0]) | u32::from(b[1]) << 8 | u32::from(b[2]) << 16;
            assert_eq!(branch_target(pc, insn), Some(want), "{name} at {pc:#x}");
            assert_eq!(l32r_literal(pc, insn), None, "{name}");
            assert_eq!(call_target(pc, insn), None, "{name}");
        }
        // ENTRY (op0 6, n 3, m 0) and a narrow MOVI.N (op0 0xC, m 0) are not.
        assert_eq!(branch_target(0x4200_0000, 0x0041_36), None);
        assert_eq!(branch_target(0x4200_0000, 0x0000_0c), None);
    }

    /// The blobs' branches carry `.text + <input offset>`: the target is the
    /// instruction's, never the start of `.text` (where the pools are).
    #[test]
    fn a_stale_branch_relocation_targets_the_branch_not_the_section_start() {
        let insn = 0xf6 | 0x22 << 8 | 0x04 << 16; // bgeui a2, 2, +8
        let t = resolve(
            20,
            0x4228_51e4,
            None,
            Some(insn),
            &HashMap::new(),
            0x4206_0618,
        );
        assert_eq!(t, Target::At(0x4228_51ec));
    }

    #[test]
    fn an_asm_expand_marker_is_no_edge() {
        for r_type in [R_XTENSA_ASM_EXPAND, R_XTENSA_ASM_SIMPLIFY] {
            let t = resolve(
                r_type,
                0x4229_33d8,
                None,
                None,
                &HashMap::new(),
                0x4206_0100,
            );
            assert_eq!(t, Target::None);
        }
    }

    #[test]
    fn a_narrowed_call_is_an_edge_to_its_target() {
        let t = resolve(20, 0x4205_0095, None, Some(CALL8), &HashMap::new(), 0);
        assert_eq!(t, Target::At(0x4209_22dc));
    }
}
