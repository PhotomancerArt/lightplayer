//! M6 stretch prototype (feature = `reloc`): parse relocatable Xtensa `.o`
//! files, lay them out in the emulator's memory map, resolve symbols across
//! objects, and apply the relocation subset that esp-toolchain-assembled small
//! objects actually carry — proving the builtins-object linking path is
//! tractable. **Prototype quality**: a real linker (GC, relaxation, COMDAT,
//! archives, weak-preemption rules …) is explicitly out of scope.
//!
//! Supported relocation types (Xtensa ELF psABI numbering, via `object::elf`):
//!
//! - `R_XTENSA_NONE` — ignored.
//! - `R_XTENSA_32` — absolute 32-bit word: `*loc = S + A + *loc` (the existing
//!   word participates because the Xtensa toolchain historically treats this
//!   reloc as partial-in-place; gas emits zeroed contents with the addend in
//!   `r_addend`, so both conventions are satisfied).
//! - `R_XTENSA_ASM_EXPAND` — assembler annotation ("this call/l32r was left
//!   long"); only meaningful to a relaxing linker, so a no-op here.
//! - `R_XTENSA_SLOT0_OP` — patch the PC-relative operand encoded *inside* the
//!   instruction slot at the reloc offset: the instruction is decoded with
//!   `lp-xt-inst`, the operand field is recomputed from the resolved target
//!   and the instruction's own PC formula, and the slot is re-encoded.
//!   Handled instruction forms: `call0/4/8/12`, `j`, `l32r`, and all decoded
//!   conditional branches (RRI8, BRI12, and the narrow `beqz.n`/`bnez.n`).
//!
//! Everything else (`R_XTENSA_DIFF*`, `R_XTENSA_SLOT1..14_OP`, `*_ALT`, PLT /
//! GOT / TLS kinds, …) is rejected with an explicit error naming the type.
//!
//! # Provenance
//!
//! Relocation type numbers and the `S + A` semantics are facts from the Xtensa
//! ELF psABI relocation appendix (see `oss/xtensa-docs`). What
//! `R_XTENSA_SLOT0_OP` *means* was understood by reading binutils'
//! `elf32-xtensa` behaviorally and by diffing GNU `ld` output on the fixture
//! objects — **no binutils code was copied or transliterated** (GPL,
//! behavioral reference only; see
//! `docs/adr/2026-07-28-license-provenance-discipline.md`). The operand
//! encodings themselves come from `lp-xt-inst` (LLVM-derived, Apache-2.0
//! w/ LLVM-exception) and the PC formulas from the Xtensa ISA Reference
//! Manual, cross-checked against `xtensa-esp32s3-elf-objdump`.

use crate::host::{GuestHost, GuestRun};
use lp_xt_emu::emu::{CODE_DBUS_BASE, CODE_REGION_LEN};
use lp_xt_emu::{Emulator, NoopTracer};
use lp_xt_inst::Inst;
use object::elf::{
    R_XTENSA_32, R_XTENSA_ASM_EXPAND, R_XTENSA_NONE, R_XTENSA_SLOT0_OP, SHF_ALLOC, SHF_EXECINSTR,
};
use object::{
    Architecture, Object, ObjectSection, ObjectSymbol, RelocationFlags, RelocationTarget,
    SectionIndex,
};
use std::collections::HashMap;

/// Where linked text (and its `.literal` pools) goes: the I-bus alias of the
/// first half of the emulator's SRAM1 code region (matches `fixtures/link.ld`).
pub const TEXT_BASE: u32 = CODE_DBUS_BASE + 0x006F_0000; // Memory::ibus_alias
/// Where data / bss goes: the D-bus second half of the same region.
pub const DATA_BASE: u32 = CODE_DBUS_BASE + (CODE_REGION_LEN as u32) / 2;
/// Capacity of each half.
const REGION_HALF: u32 = (CODE_REGION_LEN as u32) / 2;

/// Why linking a set of `.o` files failed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LinkError {
    /// The `object` crate rejected a file, or a structural read failed.
    Parse(String),
    /// An input is not a little-endian Xtensa ELF32 relocatable object.
    NotXtensaRelocatable { object_index: usize, detail: String },
    /// The laid-out sections exceed a region's capacity.
    RegionOverflow { region: &'static str, need: u32 },
    /// The same global symbol is defined by more than one object.
    DuplicateSymbol { name: String },
    /// A referenced symbol is defined by no input object.
    UndefinedSymbol { name: String },
    /// A relocation carries an implicit (in-section, SHT_REL) addend — the
    /// Xtensa toolchain emits RELA; anything else is out of scope.
    ImplicitAddend { section: String },
    /// A relocation type this prototype does not implement.
    UnsupportedReloc { r_type: u32, name: &'static str },
    /// A relocation's target could not be reduced to an address.
    BadRelocTarget { detail: String },
    /// A relocation site fell outside the bytes we placed.
    BadRelocOffset { address: u32 },
    /// `R_XTENSA_SLOT0_OP` points at bytes `lp-xt-inst` cannot decode.
    Slot0Decode { address: u32 },
    /// `R_XTENSA_SLOT0_OP` points at a decoded instruction with no
    /// PC-relative slot-0 operand this prototype knows how to patch.
    Slot0Unpatchable { address: u32, instruction: String },
    /// The recomputed operand does not fit the instruction's field (range or
    /// alignment).
    OperandRange {
        address: u32,
        target: u32,
        detail: &'static str,
    },
    /// The requested entry symbol is not defined by any input.
    NoEntrySymbol { name: String },
    /// A linked segment does not fit the emulator's modeled memory.
    Unmapped { vaddr: u32 },
}

impl core::fmt::Display for LinkError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LinkError::Parse(e) => write!(f, "object parse error: {e}"),
            LinkError::NotXtensaRelocatable {
                object_index,
                detail,
            } => write!(
                f,
                "input #{object_index} is not a little-endian Xtensa ELF32 relocatable object \
                 ({detail})"
            ),
            LinkError::RegionOverflow { region, need } => {
                write!(f, "{region} region overflow (need {need:#x} bytes)")
            }
            LinkError::DuplicateSymbol { name } => {
                write!(f, "duplicate global symbol definition: {name}")
            }
            LinkError::UndefinedSymbol { name } => write!(f, "undefined symbol: {name}"),
            LinkError::ImplicitAddend { section } => write!(
                f,
                "REL (implicit-addend) relocations in {section}; only RELA is supported"
            ),
            LinkError::UnsupportedReloc { r_type, name } => {
                write!(f, "unsupported relocation type {r_type} ({name})")
            }
            LinkError::BadRelocTarget { detail } => {
                write!(f, "unsupported relocation target: {detail}")
            }
            LinkError::BadRelocOffset { address } => {
                write!(f, "relocation site {address:#010x} outside placed sections")
            }
            LinkError::Slot0Decode { address } => write!(
                f,
                "R_XTENSA_SLOT0_OP at {address:#010x}: lp-xt-inst cannot decode the instruction"
            ),
            LinkError::Slot0Unpatchable {
                address,
                instruction,
            } => write!(
                f,
                "R_XTENSA_SLOT0_OP at {address:#010x}: no patchable PC-relative operand in \
                 `{instruction}`"
            ),
            LinkError::OperandRange {
                address,
                target,
                detail,
            } => write!(
                f,
                "relocation at {address:#010x} targeting {target:#010x}: {detail}"
            ),
            LinkError::NoEntrySymbol { name } => {
                write!(f, "entry symbol {name} not defined by any input object")
            }
            LinkError::Unmapped { vaddr } => write!(
                f,
                "linked segment at {vaddr:#010x} does not fit the emulator's modeled memory"
            ),
        }
    }
}

impl std::error::Error for LinkError {}

/// Name a relocation type for error messages (Xtensa ELF psABI numbering).
fn reloc_name(r_type: u32) -> &'static str {
    use object::elf as e;
    match r_type {
        e::R_XTENSA_NONE => "R_XTENSA_NONE",
        e::R_XTENSA_32 => "R_XTENSA_32",
        e::R_XTENSA_RTLD => "R_XTENSA_RTLD",
        e::R_XTENSA_GLOB_DAT => "R_XTENSA_GLOB_DAT",
        e::R_XTENSA_JMP_SLOT => "R_XTENSA_JMP_SLOT",
        e::R_XTENSA_RELATIVE => "R_XTENSA_RELATIVE",
        e::R_XTENSA_PLT => "R_XTENSA_PLT",
        e::R_XTENSA_OP0 | e::R_XTENSA_OP1 | e::R_XTENSA_OP2 => "R_XTENSA_OPn (legacy)",
        e::R_XTENSA_ASM_EXPAND => "R_XTENSA_ASM_EXPAND",
        e::R_XTENSA_ASM_SIMPLIFY => "R_XTENSA_ASM_SIMPLIFY",
        e::R_XTENSA_32_PCREL => "R_XTENSA_32_PCREL",
        e::R_XTENSA_GNU_VTINHERIT => "R_XTENSA_GNU_VTINHERIT",
        e::R_XTENSA_GNU_VTENTRY => "R_XTENSA_GNU_VTENTRY",
        e::R_XTENSA_DIFF8 => "R_XTENSA_DIFF8",
        e::R_XTENSA_DIFF16 => "R_XTENSA_DIFF16",
        e::R_XTENSA_DIFF32 => "R_XTENSA_DIFF32",
        21..=34 => "R_XTENSA_SLOT1..14_OP (FLIX)",
        35..=49 => "R_XTENSA_SLOT0..14_ALT",
        _ => "<unknown>",
    }
}

// --- image regions -----------------------------------------------------------

/// One contiguous output region being filled front-to-back.
struct Region {
    name: &'static str,
    base: u32,
    limit: u32,
    bytes: Vec<u8>,
}

impl Region {
    fn new(name: &'static str, base: u32, len: u32) -> Region {
        Region {
            name,
            base,
            limit: base + len,
            bytes: Vec::new(),
        }
    }

    /// Align the cursor, copy `data`, zero-pad to `size` (bss tails), and
    /// return the assigned address.
    fn place(&mut self, align: u32, data: &[u8], size: u32) -> Result<u32, LinkError> {
        let align = align.max(4);
        while !(self.base + self.bytes.len() as u32).is_multiple_of(align) {
            self.bytes.push(0);
        }
        let addr = self.base + self.bytes.len() as u32;
        if addr.checked_add(size).is_none_or(|end| end > self.limit) {
            return Err(LinkError::RegionOverflow {
                region: self.name,
                need: size,
            });
        }
        self.bytes.extend_from_slice(data);
        self.bytes
            .resize((addr - self.base) as usize + size as usize, 0);
        Ok(addr)
    }

    fn slice_mut(&mut self, addr: u32, len: usize) -> Option<&mut [u8]> {
        let start = addr.checked_sub(self.base)? as usize;
        let end = start.checked_add(len)?;
        self.bytes.get_mut(start..end)
    }
}

/// A fully linked, relocated image ready to load into the emulator.
pub struct LinkedImage {
    /// Text (+ literal pools) at [`TEXT_BASE`].
    pub text: Vec<u8>,
    /// Data / rodata / zeroed bss at [`DATA_BASE`].
    pub data: Vec<u8>,
    symbols: HashMap<String, u32>,
}

impl core::fmt::Debug for LinkedImage {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LinkedImage")
            .field("text_len", &self.text.len())
            .field("data_len", &self.data.len())
            .field("symbols", &self.symbols.len())
            .finish()
    }
}

impl LinkedImage {
    /// Address of a defined global symbol.
    pub fn symbol(&self, name: &str) -> Option<u32> {
        self.symbols.get(name).copied()
    }

    /// Copy both regions into the emulator's memory (text at its I-bus alias,
    /// exactly like the fixture linker script lays things out).
    pub fn load_into(&self, emu: &mut Emulator) -> Result<(), LinkError> {
        for (base, bytes) in [(TEXT_BASE, &self.text), (DATA_BASE, &self.data)] {
            for (i, &b) in bytes.iter().enumerate() {
                emu.mem
                    .write_u8(base.wrapping_add(i as u32), b)
                    .map_err(|_| LinkError::Unmapped { vaddr: base })?;
            }
        }
        Ok(())
    }
}

// --- slot-0 operand patching -------------------------------------------------

/// Recompute the PC-relative slot-0 operand of `inst` (located at `pc`) so it
/// refers to `target`, returning the patched instruction.
///
/// PC formulas (Xtensa ISA Reference Manual, mirrored by `lp_xt_inst::disasm`):
/// branches/`j` resolve relative to `pc + 4`; `call{0,4,8,12}` to
/// `(pc & !3) + 4` in words; `l32r` backward-only in words from
/// `(pc + 3) & !3`.
fn retarget_slot0(inst: &Inst, pc: u32, target: u32) -> Result<Inst, LinkError> {
    let err = |detail: &'static str| LinkError::OperandRange {
        address: pc,
        target,
        detail,
    };
    let t = target as i64;
    match *inst {
        Inst::Call(op, _) => {
            if !target.is_multiple_of(4) {
                return Err(err("call target not 4-byte aligned"));
            }
            let word_off = (t - ((pc & !3) as i64 + 4)) >> 2;
            if !(-(1 << 17)..(1 << 17)).contains(&word_off) {
                return Err(err("call target out of ±512 KiB range"));
            }
            Ok(Inst::Call(op, word_off as i32))
        }
        Inst::J(_) => {
            let off = t - (pc as i64 + 4);
            if !(-(1 << 17)..(1 << 17)).contains(&off) {
                return Err(err("jump target out of 18-bit range"));
            }
            Ok(Inst::J(off as i32))
        }
        Inst::L32r(rt, _) => {
            if !target.is_multiple_of(4) {
                return Err(err("l32r literal not 4-byte aligned"));
            }
            let diff = t - ((pc as i64 + 3) & !3);
            if !(-0x4_0000..=-4).contains(&diff) {
                return Err(err("l32r literal not within the backward 256 KiB window"));
            }
            Ok(Inst::L32r(rt, ((diff >> 2) & 0xffff) as u16))
        }
        Inst::BranchRr(op, s, r, _) => Ok(Inst::BranchRr(op, s, r, branch_off::<8>(pc, t)?)),
        Inst::BranchRi(op, s, v, _) => Ok(Inst::BranchRi(op, s, v, branch_off::<8>(pc, t)?)),
        Inst::BranchRiu(op, s, v, _) => Ok(Inst::BranchRiu(op, s, v, branch_off::<8>(pc, t)?)),
        Inst::BranchBiI(set, s, b, _) => Ok(Inst::BranchBiI(set, s, b, branch_off::<8>(pc, t)?)),
        Inst::BranchZ(op, s, _) => Ok(Inst::BranchZ(op, s, branch_off::<12>(pc, t)?)),
        Inst::BranchZN(nez, s, _) => {
            let off = t - (pc as i64 + 4);
            if !(0..64).contains(&off) {
                return Err(err("narrow branch target out of forward 6-bit range"));
            }
            Ok(Inst::BranchZN(nez, s, off as u32))
        }
        ref other => Err(LinkError::Slot0Unpatchable {
            address: pc,
            instruction: format!("{other:?}"),
        }),
    }
}

/// Signed `BITS`-bit branch offset relative to `pc + 4`.
fn branch_off<const BITS: u32>(pc: u32, target: i64) -> Result<i32, LinkError> {
    let off = target - (pc as i64 + 4);
    if !(-(1 << (BITS - 1))..(1 << (BITS - 1))).contains(&off) {
        return Err(LinkError::OperandRange {
            address: pc,
            target: target as u32,
            detail: "branch target out of range",
        });
    }
    Ok(off as i32)
}

// --- the linker driver -------------------------------------------------------

/// Link relocatable Xtensa `.o` files into a [`LinkedImage`]: lay out every
/// `SHF_ALLOC` section (per object: `.literal*` first — `l32r` is
/// backward-only — then executable sections into the text region; the rest
/// into the data region), resolve global symbols across objects, and apply
/// the supported relocation types.
pub fn link_objects(objects: &[&[u8]]) -> Result<LinkedImage, LinkError> {
    let mut files = Vec::with_capacity(objects.len());
    for (i, bytes) in objects.iter().enumerate() {
        let file = object::File::parse(*bytes).map_err(|e| LinkError::Parse(e.to_string()))?;
        if file.architecture() != Architecture::Xtensa
            || file.is_64()
            || !file.is_little_endian()
            || file.kind() != object::ObjectKind::Relocatable
        {
            return Err(LinkError::NotXtensaRelocatable {
                object_index: i,
                detail: format!("arch={:?}, kind={:?}", file.architecture(), file.kind()),
            });
        }
        files.push(file);
    }

    // Pass 1: assign addresses and copy section bytes.
    let mut text = Region::new("text", TEXT_BASE, REGION_HALF);
    let mut data = Region::new("data", DATA_BASE, REGION_HALF);
    let mut placed: HashMap<(usize, SectionIndex), u32> = HashMap::new();
    for (oi, file) in files.iter().enumerate() {
        // pass 0: .literal* → text; pass 1: executable → text; pass 2: rest → data.
        for pass in 0..3 {
            for sec in file.sections() {
                let object::SectionFlags::Elf { sh_flags } = sec.flags() else {
                    continue;
                };
                if sh_flags & u64::from(SHF_ALLOC) == 0 {
                    continue;
                }
                let name = sec.name().unwrap_or("");
                let is_literal = name == ".literal" || name.starts_with(".literal.");
                let is_exec = sh_flags & u64::from(SHF_EXECINSTR) != 0;
                let wanted = match pass {
                    0 => is_literal,
                    1 => is_exec && !is_literal,
                    _ => !is_exec && !is_literal,
                };
                if !wanted {
                    continue;
                }
                let bytes = sec.data().map_err(|e| LinkError::Parse(e.to_string()))?;
                let region = if pass < 2 { &mut text } else { &mut data };
                let addr = region.place(sec.align() as u32, bytes, sec.size() as u32)?;
                placed.insert((oi, sec.index()), addr);
            }
        }
    }

    // Pass 2: global symbol table.
    let mut globals: HashMap<String, u32> = HashMap::new();
    for (oi, file) in files.iter().enumerate() {
        for sym in file.symbols() {
            if !sym.is_global() {
                continue;
            }
            let Some(si) = sym.section_index() else {
                continue; // undefined (or absolute — not produced by our inputs)
            };
            let Some(&base) = placed.get(&(oi, si)) else {
                continue; // defined in a section we did not allocate
            };
            let name = sym.name().map_err(|e| LinkError::Parse(e.to_string()))?;
            let addr = base.wrapping_add(sym.address() as u32);
            if globals.insert(name.to_string(), addr).is_some() {
                return Err(LinkError::DuplicateSymbol {
                    name: name.to_string(),
                });
            }
        }
    }

    // Pass 3: apply relocations in every placed section.
    for (oi, file) in files.iter().enumerate() {
        for sec in file.sections() {
            let Some(&sec_addr) = placed.get(&(oi, sec.index())) else {
                continue;
            };
            for (off, reloc) in sec.relocations() {
                let RelocationFlags::Elf { r_type } = reloc.flags() else {
                    return Err(LinkError::BadRelocTarget {
                        detail: "non-ELF relocation flags".to_string(),
                    });
                };
                if matches!(r_type, R_XTENSA_NONE | R_XTENSA_ASM_EXPAND) {
                    continue;
                }
                if reloc.has_implicit_addend() {
                    return Err(LinkError::ImplicitAddend {
                        section: sec.name().unwrap_or("<unnamed>").to_string(),
                    });
                }
                // S: the resolved symbol address.
                let s = match reloc.target() {
                    RelocationTarget::Symbol(sym_idx) => {
                        let sym = file
                            .symbol_by_index(sym_idx)
                            .map_err(|e| LinkError::Parse(e.to_string()))?;
                        if let Some(si) = sym.section_index() {
                            let base = placed.get(&(oi, si)).ok_or(LinkError::BadRelocTarget {
                                detail: "symbol in a non-allocated section".to_string(),
                            })?;
                            base.wrapping_add(sym.address() as u32)
                        } else {
                            let name = sym.name().map_err(|e| LinkError::Parse(e.to_string()))?;
                            *globals
                                .get(name)
                                .ok_or_else(|| LinkError::UndefinedSymbol {
                                    name: name.to_string(),
                                })?
                        }
                    }
                    other => {
                        return Err(LinkError::BadRelocTarget {
                            detail: format!("{other:?}"),
                        });
                    }
                };
                let loc = sec_addr.wrapping_add(off as u32);
                let target = s.wrapping_add(reloc.addend() as u32);
                let region = if loc >= TEXT_BASE {
                    &mut text
                } else {
                    &mut data
                };
                match r_type {
                    R_XTENSA_32 => {
                        let slot = region
                            .slice_mut(loc, 4)
                            .ok_or(LinkError::BadRelocOffset { address: loc })?;
                        let existing = u32::from_le_bytes(slot.try_into().expect("4-byte slice"));
                        slot.copy_from_slice(&target.wrapping_add(existing).to_le_bytes());
                    }
                    R_XTENSA_SLOT0_OP => {
                        // Read up to 3 bytes (narrow instructions are 2).
                        let raw: Vec<u8> = match region.slice_mut(loc, 3) {
                            Some(s) => s.to_vec(),
                            None => region
                                .slice_mut(loc, 2)
                                .ok_or(LinkError::BadRelocOffset { address: loc })?
                                .to_vec(),
                        };
                        let (inst, len) = lp_xt_inst::decode(&raw)
                            .map_err(|_| LinkError::Slot0Decode { address: loc })?;
                        let patched = retarget_slot0(&inst, loc, target)?;
                        let bytes = lp_xt_inst::encode(&patched);
                        debug_assert_eq!(bytes.len(), len, "re-encode changed length");
                        region
                            .slice_mut(loc, len)
                            .ok_or(LinkError::BadRelocOffset { address: loc })?
                            .copy_from_slice(&bytes);
                    }
                    other => {
                        return Err(LinkError::UnsupportedReloc {
                            r_type: other,
                            name: reloc_name(other),
                        });
                    }
                }
            }
        }
    }

    Ok(LinkedImage {
        text: text.bytes,
        data: data.bytes,
        symbols: globals,
    })
}

/// Link `objects`, load into a fresh emulator, and run the windowed function
/// `entry_symbol(arg)` with the guest syscall ABI hosted — the two-object
/// analogue of [`crate::run_elf`].
pub fn run_linked(objects: &[&[u8]], entry_symbol: &str, arg: u32) -> Result<GuestRun, LinkError> {
    let image = link_objects(objects)?;
    let entry = image
        .symbol(entry_symbol)
        .ok_or_else(|| LinkError::NoEntrySymbol {
            name: entry_symbol.to_string(),
        })?;
    let mut emu = Emulator::new();
    emu.step_budget = 50_000_000; // match run_elf's compiled-code budget
    image.load_into(&mut emu)?;
    let mut host = GuestHost::default();
    let outcome = emu.run_loaded(entry, arg, &mut NoopTracer, &mut host);
    Ok(GuestRun {
        outcome,
        output: host.output,
        exit_code: host.exit_code,
        panic: host.panic,
    })
}

/// The text base really is the I-bus alias the emulator maps.
#[cfg(test)]
mod tests {
    use super::*;
    use lp_xt_emu::memory::Memory;
    use lp_xt_inst::disasm::l32r_target;
    use lp_xt_inst::{BrZ, CallOp, Reg};

    #[test]
    fn text_base_matches_emulator_alias() {
        assert_eq!(TEXT_BASE, Memory::ibus_alias(CODE_DBUS_BASE));
    }

    /// call8 field math reproduces the hardware target formula
    /// `(pc & !3) + 4 + (field << 2)` for both directions.
    #[test]
    fn retarget_call() {
        for (pc, target) in [
            (0x4037_800d_u32, 0x4037_8018_u32), // the GNU-ld probe case
            (0x4037_9000, 0x4037_8000),
            (0x4037_8001, 0x4037_9ffc),
        ] {
            let patched = retarget_slot0(&Inst::Call(CallOp::Call8, 0), pc, target).unwrap();
            let Inst::Call(CallOp::Call8, field) = patched else {
                panic!("wrong variant")
            };
            assert_eq!(
                (pc & !3).wrapping_add(4).wrapping_add((field as u32) << 2),
                target
            );
        }
    }

    #[test]
    fn retarget_call_rejects_unaligned() {
        let err =
            retarget_slot0(&Inst::Call(CallOp::Call8, 0), 0x4037_8000, 0x4037_8019).unwrap_err();
        assert!(matches!(err, LinkError::OperandRange { .. }));
    }

    /// l32r field math reproduces `lp_xt_inst::disasm::l32r_target`.
    #[test]
    fn retarget_l32r() {
        let pc = 0x4037_8010_u32;
        let target = 0x4037_8000_u32;
        let patched = retarget_slot0(&Inst::L32r(Reg::new(3), 0), pc, target).unwrap();
        let Inst::L32r(_, field) = patched else {
            panic!("wrong variant")
        };
        assert_eq!(field, 0xfffc); // the GNU-ld probe emitted exactly this
        assert_eq!(l32r_target(pc, field), target);
    }

    #[test]
    fn retarget_l32r_rejects_forward() {
        let err =
            retarget_slot0(&Inst::L32r(Reg::new(3), 0), 0x4037_8000, 0x4037_8100).unwrap_err();
        assert!(matches!(
            err,
            LinkError::OperandRange { detail, .. }
                if detail.contains("backward")
        ));
    }

    #[test]
    fn retarget_branch_range() {
        let pc = 0x4037_8000_u32;
        let ok =
            retarget_slot0(&Inst::BranchZ(BrZ::Beqz, Reg::new(2), 0), pc, pc + 4 + 100).unwrap();
        assert!(matches!(ok, Inst::BranchZ(BrZ::Beqz, _, 100)));
        let err =
            retarget_slot0(&Inst::BranchZ(BrZ::Beqz, Reg::new(2), 0), pc, pc + 5000).unwrap_err();
        assert!(matches!(err, LinkError::OperandRange { .. }));
    }

    /// A non-PC-relative instruction under SLOT0_OP is rejected, not mangled.
    #[test]
    fn slot0_rejects_non_pcrel() {
        let err = retarget_slot0(&Inst::Movi(Reg::new(2), 0), 0x4037_8000, 0).unwrap_err();
        assert!(matches!(err, LinkError::Slot0Unpatchable { .. }));
    }
}
