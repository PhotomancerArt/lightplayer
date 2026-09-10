//! Instruction-level tests for RISC-V emulator.
//!
//! These tests verify individual instruction decoding, encoding, and execution.

use lp_emu_core::StepResult;
use lp_riscv_emu::Riscv32Emulator;
use lp_riscv_inst::{Gpr, Inst, decode_instruction, encode};

#[test]
fn test_fence_i_decode_encode() {
    // Test FENCE.I decoding (per RISC-V spec: imm[11:0]=0x001)
    let inst = decode_instruction(0x0010100f).expect("Failed to decode FENCE.I");
    match inst {
        Inst::FenceI => {}
        _ => panic!("Expected FenceI, got {inst:?}"),
    }

    // Test FENCE.I encoding
    let encoded = encode::fence_i();
    assert_eq!(encoded, 0x0010100f);

    // Round-trip test
    let decoded = decode_instruction(encoded).expect("Failed to decode encoded FENCE.I");
    match decoded {
        Inst::FenceI => {}
        _ => panic!("Expected FenceI after round-trip, got {decoded:?}"),
    }
}

#[test]
fn test_fence_i_execution() {
    // Create a minimal emulator with FENCE.I instruction (per RISC-V spec: imm[11:0]=0x001)
    let code: Vec<u8> = vec![
        0x0f, 0x10, 0x10, 0x00, // fence.i (little-endian: 0x0010100f)
        0x73, 0x00, 0x10, 0x00, // ebreak (halt) (little-endian)
    ];
    let ram = vec![0u8; 1024];

    let mut emu = Riscv32Emulator::new(code, ram);

    // Execute FENCE.I - should be a no-op and continue
    let result = emu.step();
    assert!(result.is_ok(), "FENCE.I execution should succeed");
    match result.unwrap() {
        StepResult::Continue => {}
        _ => panic!("FENCE.I should continue execution"),
    }

    // Next instruction should be EBREAK
    let result = emu.step();
    assert!(result.is_ok(), "EBREAK execution should succeed");
    match result.unwrap() {
        StepResult::Halted => {}
        _ => panic!("EBREAK should halt execution"),
    }
}

#[test]
fn test_fence_vs_fence_i() {
    // Verify FENCE and FENCE.I are distinguished correctly
    let fence = decode_instruction(0x0000000f).expect("Failed to decode FENCE");
    match fence {
        Inst::Fence => {}
        _ => panic!("Expected Fence, got {fence:?}"),
    }

    let fence_i = decode_instruction(0x0010100f).expect("Failed to decode FENCE.I");
    match fence_i {
        Inst::FenceI => {}
        _ => panic!("Expected FenceI, got {fence_i:?}"),
    }

    // They should be different
    assert_ne!(fence, fence_i);
}

#[test]
fn test_atomic_instructions() {
    // Test that atomic instructions decode correctly
    // LR.W: 0x1000252f (lr.w a0, (zero))
    let lr_w = decode_instruction(0x1000252f).expect("Failed to decode LR.W");
    match lr_w {
        Inst::LrW { rd, rs1 } => {
            assert_eq!(rd, Gpr::A0);
            assert_eq!(rs1, Gpr::Zero);
        }
        _ => panic!("Expected LrW, got {lr_w:?}"),
    }

    // SC.W: 0x1800252f (sc.w a0, zero, (zero))
    let sc_w = decode_instruction(0x1800252f).expect("Failed to decode SC.W");
    match sc_w {
        Inst::ScW { rd, rs1, rs2 } => {
            assert_eq!(rd, Gpr::A0);
            assert_eq!(rs1, Gpr::Zero);
            assert_eq!(rs2, Gpr::Zero);
        }
        _ => panic!("Expected ScW, got {sc_w:?}"),
    }
}

#[test]
fn test_compressed_instructions() {
    // Test compressed instruction decoding
    // C.ADDI: 0x0001 (c.addi x0, 0) - but this is actually C.NOP
    // C.NOP: 0x0001
    let c_nop = decode_instruction(0x0001).expect("Failed to decode C.NOP");
    match c_nop {
        Inst::CNop => {}
        _ => panic!("Expected CNop, got {c_nop:?}"),
    }
}

#[test]
fn test_division_by_zero() {
    // Test that division by zero returns correct result
    let code: Vec<u8> = vec![
        // li a0, 10
        0x13, 0x05, 0xa0, 0x00, // addi a0, zero, 10 (little-endian)
        // li a1, 0
        0x93, 0x05, 0x00, 0x00, // addi a1, zero, 0 (little-endian)
        // div a2, a0, a1 (should return -1 per RISC-V spec)
        0x33, 0x46, 0xb5, 0x02, // div a2, a0, a1 (little-endian: 0x02b54633, funct3=0x4)
        0x73, 0x00, 0x10, 0x00, // ebreak (little-endian)
    ];
    let ram = vec![0u8; 1024];

    let mut emu = Riscv32Emulator::new(code, ram);

    // Execute until halt
    loop {
        match emu.step() {
            Ok(StepResult::Halted) => break,
            Ok(_) => continue,
            Err(e) => panic!("Emulator error: {e:?}"),
        }
    }

    // Check result: division by zero should return -1
    let a2 = emu.get_register(Gpr::A2);
    assert_eq!(a2, -1, "Division by zero should return -1 per RISC-V spec");
}

#[test]
fn test_unaligned_access() {
    // Test that unaligned memory access is detected
    let code: Vec<u8> = vec![
        // li a0, 1 (unaligned address)
        0x13, 0x05, 0x10, 0x00, // addi a0, zero, 1 (little-endian)
        // lw a1, 0(a0) - should fail (unaligned)
        0x03, 0x25, 0x05, 0x00, // lw a1, 0(a0) (little-endian)
        0x73, 0x00, 0x10, 0x00, // ebreak (little-endian)
    ];
    let ram = vec![0u8; 1024];

    let mut emu = Riscv32Emulator::new(code, ram);

    // First instruction should succeed
    let result = emu.step();
    assert!(result.is_ok(), "Setting register should succeed");

    // Second instruction (unaligned load) should fail
    let result = emu.step();
    assert!(result.is_err(), "Unaligned load should fail");
}

/// `xori rd, rs1, 128` must decode as XORI for **every** immediate, including
/// 0x080.
///
/// The decoder used to carve funct12 == 0x080 out of OP-IMM funct3=100 and
/// call it `zext.h`. Those two are the same 32 bits, and the base-ISA
/// instruction is the one that actually occurs: LLVM emits `xori rd, rs, 128`
/// as the index bias of a large jump table, so the mis-decode turned a
/// `&'static str` lookup into a read of the wrong table slot. It stayed
/// invisible because `zext.h` is a no-op on the small values an enum
/// discriminant takes.
#[test]
fn xori_128_is_xori_not_zexth() {
    let encoded = encode::xori(Gpr::A0, Gpr::A0, 128);
    assert_eq!(encoded, 0x0805_4513, "xori a0, a0, 128");

    match decode_instruction(encoded).expect("decode xori a0, a0, 128") {
        Inst::Xori { rd, rs1, imm } => {
            assert_eq!(rd, Gpr::A0);
            assert_eq!(rs1, Gpr::A0);
            assert_eq!(imm, 128);
        }
        other => panic!("expected Xori, got {other:?}"),
    }
}

/// Executing that instruction must actually flip bit 7.
#[test]
fn xori_128_flips_bit_seven() {
    let code: Vec<u8> = vec![
        0x13, 0x05, 0x30, 0x00, // addi a0, zero, 3
        0x13, 0x45, 0x05, 0x08, // xori a0, a0, 128
        0x73, 0x00, 0x10, 0x00, // ebreak
    ];
    let mut emu = Riscv32Emulator::new(code, vec![0u8; 1024]);
    loop {
        match emu.step() {
            Ok(StepResult::Halted) => break,
            Ok(_) => continue,
            Err(e) => panic!("Emulator error: {e:?}"),
        }
    }
    assert_eq!(
        emu.get_register(Gpr::A0),
        3 ^ 128,
        "xori a0, a0, 128 must xor, not zero-extend"
    );
}

/// `zext.h` belongs to the OP space: RV32 spells it `pack rd, rs1, x0`.
#[test]
fn zexth_uses_the_op_encoding() {
    let encoded = encode::zexth(Gpr::A0, Gpr::A1);
    assert_eq!(encoded & 0x7f, 0x33, "zext.h is an OP (R-type) encoding");
    assert_eq!(encoded, 0x0805_C533, "zext.h a0, a1 == pack a0, a1, x0");

    match decode_instruction(encoded).expect("decode zext.h") {
        Inst::Zexth { rd, rs1 } => {
            assert_eq!(rd, Gpr::A0);
            assert_eq!(rs1, Gpr::A1);
        }
        other => panic!("expected Zexth, got {other:?}"),
    }
}

/// And it must still zero-extend when executed at its real encoding.
#[test]
fn zexth_zero_extends_halfword() {
    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&encode::lui(Gpr::A1, 0xABCDC).to_le_bytes());
    code.extend_from_slice(&encode::zexth(Gpr::A0, Gpr::A1).to_le_bytes());
    code.extend_from_slice(&[0x73, 0x00, 0x10, 0x00]); // ebreak

    let mut emu = Riscv32Emulator::new(code, vec![0u8; 1024]);
    loop {
        match emu.step() {
            Ok(StepResult::Halted) => break,
            Ok(_) => continue,
            Err(e) => panic!("Emulator error: {e:?}"),
        }
    }
    let src = emu.get_register(Gpr::A1) as u32;
    assert!(src > 0xFFFF, "fixture must set bits above the halfword");
    assert_eq!(
        emu.get_register(Gpr::A0) as u32,
        src & 0xFFFF,
        "zext.h keeps only the low 16 bits"
    );
}

// ---------------------------------------------------------------------------
// Zb* immediate forms.
//
// Every word below came out of the assembler, not out of a sibling
// instruction's shape — that is the standing lesson of
// `docs/defects/2026-07-31-zexth-encoding-steals-xori-128.md`. They were
// produced by assembling
//
//     .option arch, +zba, +zbb, +zbs, +zbkb
//     rori a0,a1,4 / rev8 a0,a1 / brev8 a0,a1 / orc.b a0,a1
//     bexti a0,a1,3 / bclri a0,a1,3 / bseti a0,a1,3 / binvi a0,a1,3
//
// for `riscv32imac-unknown-none-elf` and reading the words back with
// llvm-objdump.
// ---------------------------------------------------------------------------

/// The whole table, three ways: the word decodes to the right instruction, the
/// instruction re-encodes to the same word, and the disassembly prints the
/// mnemonic the assembler accepts.
#[test]
fn zb_immediate_forms_match_the_assembler() {
    let cases: &[(u32, Inst, &str)] = &[
        (
            0x6045_D513,
            Inst::Rori {
                rd: Gpr::A0,
                rs1: Gpr::A1,
                imm: 4,
            },
            "rori a0, a1, 4",
        ),
        (
            0x6985_D513,
            Inst::Rev8 {
                rd: Gpr::A0,
                rs1: Gpr::A1,
            },
            "rev8 a0, a1",
        ),
        (
            0x6875_D513,
            Inst::Brev8 {
                rd: Gpr::A0,
                rs1: Gpr::A1,
            },
            "brev8 a0, a1",
        ),
        (
            0x2875_D513,
            Inst::Orcb {
                rd: Gpr::A0,
                rs1: Gpr::A1,
            },
            "orc.b a0, a1",
        ),
        (
            0x4835_D513,
            Inst::Bexti {
                rd: Gpr::A0,
                rs1: Gpr::A1,
                imm: 3,
            },
            "bexti a0, a1, 3",
        ),
        (
            0x4835_9513,
            Inst::Bclri {
                rd: Gpr::A0,
                rs1: Gpr::A1,
                imm: 3,
            },
            "bclri a0, a1, 3",
        ),
        (
            0x2835_9513,
            Inst::Bseti {
                rd: Gpr::A0,
                rs1: Gpr::A1,
                imm: 3,
            },
            "bseti a0, a1, 3",
        ),
        (
            0x6835_9513,
            Inst::Binvi {
                rd: Gpr::A0,
                rs1: Gpr::A1,
                imm: 3,
            },
            "binvi a0, a1, 3",
        ),
        // The funct12 neighbours these arms sit next to, so a future edit to
        // one cannot quietly swallow another.
        (
            0x6005_9513,
            Inst::Clz {
                rd: Gpr::A0,
                rs1: Gpr::A1,
            },
            "clz a0, a1",
        ),
        (
            0x6015_9513,
            Inst::Ctz {
                rd: Gpr::A0,
                rs1: Gpr::A1,
            },
            "ctz a0, a1",
        ),
        (
            0x6025_9513,
            Inst::Cpop {
                rd: Gpr::A0,
                rs1: Gpr::A1,
            },
            "cpop a0, a1",
        ),
        (
            0x6045_9513,
            Inst::Sextb {
                rd: Gpr::A0,
                rs1: Gpr::A1,
            },
            "sext.b a0, a1",
        ),
        (
            0x6055_9513,
            Inst::Sexth {
                rd: Gpr::A0,
                rs1: Gpr::A1,
            },
            "sext.h a0, a1",
        ),
        // The base-ISA shifts that share this funct3 space, unchanged.
        (
            0x0035_9513,
            Inst::Slli {
                rd: Gpr::A0,
                rs1: Gpr::A1,
                imm: 3,
            },
            "slli a0, a1, 3",
        ),
        (
            0x0035_D513,
            Inst::Srli {
                rd: Gpr::A0,
                rs1: Gpr::A1,
                imm: 3,
            },
            "srli a0, a1, 3",
        ),
        (
            0x4035_D513,
            Inst::Srai {
                rd: Gpr::A0,
                rs1: Gpr::A1,
                imm: 3,
            },
            "srai a0, a1, 3",
        ),
    ];

    for (word, want, text) in cases {
        let got = decode_instruction(*word)
            .unwrap_or_else(|e| panic!("{text} ({word:#010x}) failed to decode: {e}"));
        assert_eq!(&got, want, "decode of {word:#010x} ({text})");
        assert_eq!(
            got.encode(),
            *word,
            "re-encode of {text}: {:#010x} != {word:#010x}",
            got.encode()
        );
        assert_eq!(got.format(), *text, "disassembly of {word:#010x}");
    }
}

/// The headline mis-transcription: funct6 0x12 is `bclri`, and the decoder
/// named it `bseti`. Nothing failed loudly, because both are real
/// instructions — a disassembly said `bseti`, and a disassemble/re-assemble
/// round trip turned a bit-clear into a bit-set at a different encoding.
#[test]
fn bclri_disassembles_as_bclri_not_bseti() {
    let bclri = 0x4835_9513u32; // bclri a0, a1, 3
    let bseti = 0x2835_9513u32; // bseti a0, a1, 3

    assert_eq!(
        decode_instruction(bclri).unwrap().format(),
        "bclri a0, a1, 3"
    );
    assert_eq!(
        decode_instruction(bseti).unwrap().format(),
        "bseti a0, a1, 3"
    );
    assert_ne!(
        decode_instruction(bclri).unwrap().encode(),
        bseti,
        "a bclri word must never re-encode as bseti"
    );
    assert_eq!(encode::bclri(Gpr::A0, Gpr::A1, 3), bclri);
    assert_eq!(encode::bseti(Gpr::A0, Gpr::A1, 3), bseti);
}

/// `rev8` is the one where encode and decode were wrong the *same* way: both
/// used the RV64 funct12 0x6b8, so any round-trip test written against the
/// crate's own encoder would have passed on a word that is not an RV32
/// instruction at all. The assembler is the only witness that catches it.
#[test]
fn rev8_uses_the_rv32_funct12() {
    let word = 0x6985_D513u32; // rev8 a0, a1 on RV32
    assert_eq!(encode::rev8(Gpr::A0, Gpr::A1), word);
    assert_eq!((word >> 20) & 0xfff, 0x698);
    assert_eq!(decode_instruction(word).unwrap().format(), "rev8 a0, a1");
}

/// Words that are RV64-only spellings, or that RV32 reserves. `llvm-objdump`
/// renders all three as `.word` for `riscv32` with every Zb extension enabled,
/// so an RV32 disassembler must not put a mnemonic on them.
#[test]
fn rv64_only_bitmanip_words_do_not_decode_on_rv32() {
    for (word, what) in [
        (0x6B85_D513u32, "rev8 a0, a1 as spelled on RV64"),
        (0x0835_9513u32, "slli.uw a0, a1, 3 (RV64-only Zba)"),
        (0x4A35_9513u32, "bclri a0, a1, 35 (shamt >= 32 needs RV64)"),
    ] {
        let decoded = decode_instruction(word);
        assert!(
            decoded.is_err(),
            "{what} ({word:#010x}) is reserved on RV32 but decoded as {:?}",
            decoded.ok()
        );
    }
}

/// A 5-bit shift amount is all RV32 has: bit 25 belongs to funct7. Encoding a
/// larger amount must not walk into the neighbouring instruction — `bclri`
/// with shamt 35 would otherwise emit funct7 0x25, a reserved word.
#[test]
fn zb_shift_immediates_stay_within_five_bits() {
    for shamt in 0..32i32 {
        for (name, encoded) in [
            ("bclri", encode::bclri(Gpr::A0, Gpr::A1, shamt)),
            ("bseti", encode::bseti(Gpr::A0, Gpr::A1, shamt)),
            ("binvi", encode::binvi(Gpr::A0, Gpr::A1, shamt)),
            ("bexti", encode::bexti(Gpr::A0, Gpr::A1, shamt)),
            ("rori", encode::rori(Gpr::A0, Gpr::A1, shamt)),
        ] {
            let decoded = decode_instruction(encoded)
                .unwrap_or_else(|e| panic!("{name} shamt={shamt} failed to decode: {e}"));
            assert_eq!(
                decoded.encode(),
                encoded,
                "{name} shamt={shamt} does not round-trip"
            );
            assert!(
                decoded.format().starts_with(name),
                "{name} shamt={shamt} disassembled as {}",
                decoded.format()
            );
        }
    }

    // Bit 25 is funct7, so an out-of-range amount is masked rather than
    // carried into it.
    assert_eq!(
        encode::bclri(Gpr::A0, Gpr::A1, 35),
        encode::bclri(Gpr::A0, Gpr::A1, 3),
        "shamt 35 must not become the reserved funct7 0x25"
    );
}
