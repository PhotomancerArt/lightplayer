//! Loader + hosted-run tests over synthetic ELFs.
//!
//! The ELF images here are built in-test from the ELF32 structure layouts
//! (public-spec facts: Ehdr/Phdr/Shdr/Rela field order and sizes); the guest
//! code inside them is assembled with `lp-xt-inst`'s own encoder — never
//! hand-recalled bytes. Toolchain-compiled fixture coverage lives in
//! `tests/fixtures.rs`.

use lp_xt_elf::{ElfError, XtensaElf, abi, run_elf};
use lp_xt_emu::emu::CODE_DBUS_BASE;
use lp_xt_emu::memory::Memory;
use lp_xt_emu::{Emulator, RunOutcome};
use lp_xt_inst::{CallOp, Inst, NullaryOp, Reg, StoreOp, encode};

// --- tiny ELF32 writer (little-endian) --------------------------------------

const EM_XTENSA: u16 = 94;
const ET_EXEC: u16 = 2;
const ET_REL: u16 = 1;

fn le16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn le32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

struct SynthElf {
    e_type: u16,
    machine: u16,
    entry: u32,
    /// (vaddr, file bytes, memsz)
    segments: Vec<(u32, Vec<u8>, u32)>,
    /// Append a section-header table containing an SHT_RELA section.
    with_rela: bool,
}

impl SynthElf {
    fn exec(entry: u32) -> SynthElf {
        SynthElf {
            e_type: ET_EXEC,
            machine: EM_XTENSA,
            entry,
            segments: Vec::new(),
            with_rela: false,
        }
    }

    fn seg(mut self, vaddr: u32, data: Vec<u8>, memsz: u32) -> SynthElf {
        self.segments.push((vaddr, data, memsz));
        self
    }

    fn build(&self) -> Vec<u8> {
        const EHSIZE: u32 = 52;
        const PHENT: u32 = 32;
        const SHENT: u32 = 40;
        let phnum = self.segments.len() as u32;
        let phoff = EHSIZE;
        let mut data_off = phoff + phnum * PHENT;

        // Segment data offsets.
        let mut seg_offs = Vec::new();
        for (_, data, _) in &self.segments {
            seg_offs.push(data_off);
            data_off += data.len() as u32;
        }

        // Optional section headers: NULL, .text, .rela.text, .shstrtab.
        let shstrtab: &[u8] = b"\0.text\0.rela.text\0.shstrtab\0";
        let (shoff, shnum, shstrndx, rela_off, shstr_off) = if self.with_rela {
            let rela_off = data_off;
            let shstr_off = rela_off + 12; // one Elf32_Rela entry
            let shoff = shstr_off + shstrtab.len() as u32;
            (shoff, 4u16, 3u16, rela_off, shstr_off)
        } else {
            (0, 0, 0, 0, 0)
        };

        let mut out = Vec::new();
        // e_ident
        out.extend_from_slice(&[0x7f, b'E', b'L', b'F', 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        le16(&mut out, self.e_type);
        le16(&mut out, self.machine);
        le32(&mut out, 1); // e_version
        le32(&mut out, self.entry);
        le32(&mut out, if phnum > 0 { phoff } else { 0 });
        le32(&mut out, shoff);
        le32(&mut out, 0); // e_flags
        le16(&mut out, EHSIZE as u16);
        le16(&mut out, PHENT as u16);
        le16(&mut out, phnum as u16);
        le16(&mut out, SHENT as u16);
        le16(&mut out, shnum);
        le16(&mut out, shstrndx);
        assert_eq!(out.len() as u32, EHSIZE);

        // Program headers (PT_LOAD).
        for ((vaddr, data, memsz), off) in self.segments.iter().zip(&seg_offs) {
            le32(&mut out, 1); // PT_LOAD
            le32(&mut out, *off);
            le32(&mut out, *vaddr);
            le32(&mut out, *vaddr); // p_paddr
            le32(&mut out, data.len() as u32);
            le32(&mut out, *memsz);
            le32(&mut out, 7); // RWX
            le32(&mut out, 4); // p_align
        }

        // Segment data.
        for (_, data, _) in &self.segments {
            out.extend_from_slice(data);
        }

        if self.with_rela {
            // One Elf32_Rela entry: r_offset, r_info (sym 0, type 1), r_addend.
            le32(&mut out, 0);
            le32(&mut out, 1);
            le32(&mut out, 0);
            out.extend_from_slice(shstrtab);

            // Section headers.
            let sh = |name: u32,
                      ty: u32,
                      addr: u32,
                      off: u32,
                      size: u32,
                      link: u32,
                      info: u32,
                      entsize: u32,
                      out: &mut Vec<u8>| {
                le32(out, name);
                le32(out, ty);
                le32(out, 0); // sh_flags
                le32(out, addr);
                le32(out, off);
                le32(out, size);
                le32(out, link);
                le32(out, info);
                le32(out, 4); // sh_addralign
                le32(out, entsize);
            };
            assert_eq!(out.len() as u32, shoff);
            sh(0, 0, 0, 0, 0, 0, 0, 0, &mut out); // NULL
            let (text_vaddr, text_len) = self
                .segments
                .first()
                .map(|(v, d, _)| (*v, d.len() as u32))
                .unwrap_or((0, 0));
            let text_off = seg_offs.first().copied().unwrap_or(0);
            // .text (SHT_PROGBITS)
            sh(1, 1, text_vaddr, text_off, text_len, 0, 0, 0, &mut out);
            // .rela.text (SHT_RELA), sh_info = 1 (.text)
            sh(7, 4, 0, rela_off, 12, 0, 1, 12, &mut out);
            // .shstrtab (SHT_STRTAB)
            sh(
                18,
                3,
                0,
                shstr_off,
                shstrtab.len() as u32,
                0,
                0,
                0,
                &mut out,
            );
        }

        out
    }
}

// --- guest code assembled with lp-xt-inst ------------------------------------

const IBUS_TEXT: u32 = CODE_DBUS_BASE + 0x006F_0000; // Memory::ibus_alias(CODE_DBUS_BASE)

fn asm(insts: &[Inst]) -> Vec<u8> {
    insts.iter().flat_map(encode).collect()
}

fn a(n: u8) -> Reg {
    Reg::new(n)
}

// --- tests -------------------------------------------------------------------

/// entry a1,32; movi a2,42; retw — plain windowed return, no syscalls.
#[test]
fn runs_linked_executable() {
    let text = asm(&[
        Inst::Entry(a(1), 32),
        Inst::Movi(a(2), 42),
        Inst::Nullary(NullaryOp::Retw),
    ]);
    let elf = SynthElf::exec(IBUS_TEXT).seg(IBUS_TEXT, text, 9).build();
    let run = run_elf(&elf, 0).expect("load");
    assert_eq!(run.outcome, RunOutcome::Ok(42));
    assert_eq!(run.exit_code, None);
    assert!(run.output.is_empty());
    assert_eq!(run.panic, None);
}

/// Stage "Hi" on the stack, SYS_WRITE it, then SYS_EXIT(7). Exercises the
/// full syscall path: number/arg registers, result write-back, resume pc.
#[test]
fn syscall_write_and_exit() {
    let text = asm(&[
        Inst::Entry(a(1), 48),
        Inst::Movi(a(4), b'H' as i32),
        Inst::Store(StoreOp::S8i, a(4), a(1), 0),
        Inst::Movi(a(4), b'i' as i32),
        Inst::Store(StoreOp::S8i, a(4), a(1), 1),
        Inst::Movi(a(2), abi::SYS_WRITE as i32),
        Inst::MovN(a(3), a(1)),
        Inst::Movi(a(4), 2),
        Inst::Nullary(NullaryOp::Syscall),
        // SYS_WRITE's a2 result (bytes written = 2) becomes part of the exit
        // code, proving the result write-back: exit code = 2 + 5 = 7.
        Inst::Addi(a(3), a(2), 5),
        Inst::Movi(a(2), abi::SYS_EXIT as i32),
        Inst::Nullary(NullaryOp::Syscall),
        // Not reached: SYS_EXIT terminates the run.
        Inst::Nullary(NullaryOp::Retw),
    ]);
    let elf = SynthElf::exec(IBUS_TEXT)
        .seg(IBUS_TEXT, text.clone(), text.len() as u32)
        .build();
    let run = run_elf(&elf, 0).expect("load");
    assert_eq!(run.outcome, RunOutcome::Ok(7));
    assert_eq!(run.exit_code, Some(7));
    assert_eq!(run.output_str(), "Hi");
    assert_eq!(run.panic, None);
}

/// A call to a second windowed function that SYS_WRITEs — the syscall sees the
/// callee's window (a2..), not the caller's.
#[test]
fn syscall_from_callee_window() {
    // f: entry a1,32; call8 g; movi a2, 9; retw
    // g: entry a1,32; store 'X' to stack; write; retw
    let f = [
        Inst::Entry(a(1), 32),
        // call8 at pc = base+3: target = ((base+3) & !3) + (2 << 2) + 4 = base+12 = g.
        Inst::Call(CallOp::Call8, 2),
        Inst::Movi(a(2), 9),
        Inst::Nullary(NullaryOp::Retw),
    ];
    // Compute g's offset from encoded lengths instead of guessing.
    let f_bytes = asm(&f);
    assert_eq!(f_bytes.len(), 12, "call8 target arithmetic relies on this");
    let g = [
        Inst::Entry(a(1), 32),
        Inst::Movi(a(4), b'X' as i32),
        Inst::Store(StoreOp::S8i, a(4), a(1), 0),
        Inst::Movi(a(2), abi::SYS_WRITE as i32),
        Inst::MovN(a(3), a(1)),
        Inst::Movi(a(4), 1),
        Inst::Nullary(NullaryOp::Syscall),
        Inst::Nullary(NullaryOp::Retw),
    ];
    let mut text = f_bytes;
    text.extend(asm(&g));
    let len = text.len() as u32;
    let elf = SynthElf::exec(IBUS_TEXT).seg(IBUS_TEXT, text, len).build();
    let run = run_elf(&elf, 0).expect("load");
    assert_eq!(run.outcome, RunOutcome::Ok(9));
    assert_eq!(run.output_str(), "X");
}

/// SYS_PANIC surfaces the message and the synthesized exit code.
#[test]
fn syscall_panic() {
    let text = asm(&[
        Inst::Entry(a(1), 48),
        Inst::Movi(a(4), b'o' as i32),
        Inst::Store(StoreOp::S8i, a(4), a(1), 0),
        Inst::Store(StoreOp::S8i, a(4), a(1), 1),
        Inst::Movi(a(2), abi::SYS_PANIC as i32),
        Inst::MovN(a(3), a(1)),
        Inst::Movi(a(4), 2),
        Inst::Nullary(NullaryOp::Syscall),
        Inst::Nullary(NullaryOp::Retw),
    ]);
    let len = text.len() as u32;
    let elf = SynthElf::exec(IBUS_TEXT).seg(IBUS_TEXT, text, len).build();
    let run = run_elf(&elf, 0).expect("load");
    assert_eq!(run.outcome, RunOutcome::Ok(abi::PANIC_EXIT_CODE));
    assert_eq!(run.exit_code, Some(abi::PANIC_EXIT_CODE));
    assert_eq!(run.panic.as_deref(), Some("oo"));
}

/// `.bss` tails (p_memsz > p_filesz) are zeroed even over pre-existing junk.
#[test]
fn bss_tail_is_zeroed() {
    let dbus_data = CODE_DBUS_BASE + 0x1000;
    let elf = SynthElf::exec(IBUS_TEXT)
        .seg(IBUS_TEXT, asm(&[Inst::Nullary(NullaryOp::Retw)]), 3)
        .seg(dbus_data, vec![0xAA, 0xBB], 16)
        .build();
    let mut emu = Emulator::new();
    for i in 0..16 {
        emu.mem.write_u8(dbus_data + i, 0x55).unwrap();
    }
    let parsed = XtensaElf::parse(&elf).expect("parse");
    parsed.load_into(&mut emu).expect("load");
    assert_eq!(emu.mem.read_u8(dbus_data).unwrap(), 0xAA);
    assert_eq!(emu.mem.read_u8(dbus_data + 1).unwrap(), 0xBB);
    for i in 2..16 {
        assert_eq!(emu.mem.read_u8(dbus_data + i).unwrap(), 0, "byte {i}");
    }
}

#[test]
fn rejects_wrong_machine() {
    let mut synth = SynthElf::exec(IBUS_TEXT).seg(IBUS_TEXT, vec![0; 4], 4);
    synth.machine = 243; // EM_RISCV
    let err = XtensaElf::parse(&synth.build()).unwrap_err();
    assert!(
        matches!(err, ElfError::NotXtensaElf32 { .. }),
        "got {err:?}"
    );
}

#[test]
fn rejects_relocatable_object() {
    let mut synth = SynthElf::exec(0);
    synth.e_type = ET_REL;
    let err = XtensaElf::parse(&synth.build()).unwrap_err();
    assert!(matches!(err, ElfError::NotExecutable { .. }), "got {err:?}");
}

#[test]
fn rejects_relocations() {
    let mut synth = SynthElf::exec(IBUS_TEXT).seg(IBUS_TEXT, vec![0; 8], 8);
    synth.with_rela = true;
    let err = XtensaElf::parse(&synth.build()).unwrap_err();
    match err {
        ElfError::HasRelocations { section } => assert_eq!(section, ".text"),
        other => panic!("expected HasRelocations, got {other:?}"),
    }
}

/// A segment outside the modeled memory map fails cleanly (no panic).
#[test]
fn rejects_unmapped_segment() {
    let elf = SynthElf::exec(IBUS_TEXT)
        .seg(0x6000_0000, vec![1, 2, 3, 4], 4)
        .build();
    let parsed = XtensaElf::parse(&elf).expect("parse");
    let mut emu = Emulator::new();
    let err = parsed.load_into(&mut emu).unwrap_err();
    assert!(
        matches!(
            err,
            ElfError::Unmapped {
                vaddr: 0x6000_0000,
                ..
            }
        ),
        "got {err:?}"
    );
}

/// The IBUS_TEXT constant used throughout matches the emulator's alias math.
#[test]
fn ibus_text_matches_alias() {
    assert_eq!(IBUS_TEXT, Memory::ibus_alias(CODE_DBUS_BASE));
}
