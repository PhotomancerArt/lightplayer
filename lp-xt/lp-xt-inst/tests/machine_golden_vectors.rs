//! Golden vectors for the machine-mode / privileged families M1 P1 added.
//!
//! **Every byte here is objdump-derived**: each vector was assembled from a
//! one-instruction `.S` file with **both** `xtensa-esp32-elf-as` (LX6) and
//! `xtensa-esp32s3-elf-as` (LX7) and read back with the matching `-objdump -d`.
//! Nothing is hand-recalled (spike lesson: recalls were wrong 2/3), and nothing
//! is transcribed by analogy (repo memory: *get encodings from an assembler,
//! never by analogy* — 7 of 14 arms were dead or wrong the one time that rule
//! was broken).
//!
//! ## LX6 / LX7 agreement
//!
//! The two assemblers produced **identical bytes for every vector in this
//! file**. The only LX6/LX7 divergences found across the whole probe were:
//!
//! - the user registers `EXPSTATE` (230), `F64R_LO` (234), `F64R_HI` (235) and
//!   `F64S` (236) — accepted by the LX6 assembler, rejected outright by LX7;
//! - the `f64*` double-precision-accelerator *instruction* group — LX6 only,
//!   and **not** decoded by this crate (see the crate README's residue list).
//!
//! `MEMCTL` (SR 97) is **not** an LX6/LX7 divergence: both assemblers accept
//! `rsr.memctl` / `wsr.memctl` / `xsr.memctl`, contrary to the M1 planning
//! note's expectation.
//!
//! The `SR_UR_VECTORS` table below is the full `rsr`/`wsr`/`xsr` × modelled-SR
//! and `rur`/`wur` × modelled-UR cross product, generated from the assembler's
//! own output plus `wsr.mmid` — 208 vectors, one per legal mnemonic in the space.

use lp_xt_inst::*;

fn a(n: u8) -> Reg {
    Reg::new(n)
}
fn br(n: u8) -> BReg {
    BReg::new(n)
}
fn m(n: u8) -> MReg {
    MReg::new(n)
}

/// Decode one instruction, asserting it round-trips to the exact input bytes.
#[track_caller]
fn dec(bytes: &[u8]) -> Inst {
    let (inst, len) = decode(bytes).unwrap_or_else(|e| panic!("decode {bytes:02x?}: {e}"));
    assert_eq!(len, bytes.len(), "length for {inst:?}");
    assert_eq!(encode(&inst), bytes, "round-trip for {inst:?}");
    inst
}

/// Compare this crate's disassembly of `bytes` at pc 0 against objdump's own
/// text, normalising the differences that are pure formatting:
///
/// - the mnemonic/operand separator (`\t` here, `\t` there, whitespace either
///   way);
/// - decimal vs hex immediates (`252` vs `0xfc`), compared numerically;
/// - objdump's boolean *range* rendering (`b4:b5:b6:b7`) versus the first
///   register of the range, which is what the field actually holds.
#[track_caller]
fn agrees_with_objdump(bytes: &[u8], objdump: &str) {
    let mine = format_instruction(bytes, 0);
    let (my_mnem, my_ops) = split(&mine);
    let (od_mnem, od_ops) = split(objdump);
    assert_eq!(my_mnem, od_mnem, "mnemonic for {bytes:02x?} ({mine})");
    assert_eq!(
        my_ops.len(),
        od_ops.len(),
        "operand count for {bytes:02x?}: {mine} vs {objdump}"
    );
    for (m, o) in my_ops.iter().zip(&od_ops) {
        let same = match (parse_int(m), parse_int(o)) {
            (Some(x), Some(y)) => x == y,
            _ => m == o,
        };
        assert!(
            same,
            "operand {m} vs {o} for {bytes:02x?}: {mine} / {objdump}"
        );
    }
}

fn split(s: &str) -> (String, Vec<String>) {
    let s = s.replace('\t', " ");
    let mut it = s.trim().splitn(2, ' ');
    let mnem = it.next().unwrap_or("").to_string();
    let ops = it
        .next()
        .map(|r| {
            r.split(',')
                .map(|t| t.trim().split(':').next().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default();
    (mnem, ops)
}

fn parse_int(s: &str) -> Option<i64> {
    let (neg, body) = match s.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, s),
    };
    let v = match body.strip_prefix("0x") {
        Some(h) => i64::from_str_radix(h, 16).ok()?,
        None => body.parse::<i64>().ok()?,
    };
    Some(if neg { -v } else { v })
}

// ---------------------------------------------------------------------------
// The SR / UR space
// ---------------------------------------------------------------------------

/// Source: one `.S` line per entry, e.g. `  rsr.lbeg a3`. LX6 == LX7 for all.
#[rustfmt::skip]
const SR_UR_VECTORS: &[(&[u8], &str)] = &[
    (&[0x30, 0x00, 0x03], "rsr.lbeg a3"),
    (&[0x30, 0x00, 0x13], "wsr.lbeg a3"),
    (&[0x30, 0x00, 0x61], "xsr.lbeg a3"),
    (&[0x30, 0x01, 0x03], "rsr.lend a3"),
    (&[0x30, 0x01, 0x13], "wsr.lend a3"),
    (&[0x30, 0x01, 0x61], "xsr.lend a3"),
    (&[0x30, 0x02, 0x03], "rsr.lcount a3"),
    (&[0x30, 0x02, 0x13], "wsr.lcount a3"),
    (&[0x30, 0x02, 0x61], "xsr.lcount a3"),
    (&[0x30, 0x03, 0x03], "rsr.sar a3"),
    (&[0x30, 0x03, 0x13], "wsr.sar a3"),
    (&[0x30, 0x03, 0x61], "xsr.sar a3"),
    (&[0x30, 0x04, 0x03], "rsr.br a3"),
    (&[0x30, 0x04, 0x13], "wsr.br a3"),
    (&[0x30, 0x04, 0x61], "xsr.br a3"),
    (&[0x30, 0x05, 0x03], "rsr.litbase a3"),
    (&[0x30, 0x05, 0x13], "wsr.litbase a3"),
    (&[0x30, 0x05, 0x61], "xsr.litbase a3"),
    (&[0x30, 0x0c, 0x03], "rsr.scompare1 a3"),
    (&[0x30, 0x0c, 0x13], "wsr.scompare1 a3"),
    (&[0x30, 0x0c, 0x61], "xsr.scompare1 a3"),
    (&[0x30, 0x10, 0x03], "rsr.acclo a3"),
    (&[0x30, 0x10, 0x13], "wsr.acclo a3"),
    (&[0x30, 0x10, 0x61], "xsr.acclo a3"),
    (&[0x30, 0x11, 0x03], "rsr.acchi a3"),
    (&[0x30, 0x11, 0x13], "wsr.acchi a3"),
    (&[0x30, 0x11, 0x61], "xsr.acchi a3"),
    (&[0x30, 0x20, 0x03], "rsr.m0 a3"),
    (&[0x30, 0x20, 0x13], "wsr.m0 a3"),
    (&[0x30, 0x20, 0x61], "xsr.m0 a3"),
    (&[0x30, 0x21, 0x03], "rsr.m1 a3"),
    (&[0x30, 0x21, 0x13], "wsr.m1 a3"),
    (&[0x30, 0x21, 0x61], "xsr.m1 a3"),
    (&[0x30, 0x22, 0x03], "rsr.m2 a3"),
    (&[0x30, 0x22, 0x13], "wsr.m2 a3"),
    (&[0x30, 0x22, 0x61], "xsr.m2 a3"),
    (&[0x30, 0x23, 0x03], "rsr.m3 a3"),
    (&[0x30, 0x23, 0x13], "wsr.m3 a3"),
    (&[0x30, 0x23, 0x61], "xsr.m3 a3"),
    (&[0x30, 0x48, 0x03], "rsr.windowbase a3"),
    (&[0x30, 0x48, 0x13], "wsr.windowbase a3"),
    (&[0x30, 0x48, 0x61], "xsr.windowbase a3"),
    (&[0x30, 0x49, 0x03], "rsr.windowstart a3"),
    (&[0x30, 0x49, 0x13], "wsr.windowstart a3"),
    (&[0x30, 0x49, 0x61], "xsr.windowstart a3"),
    (&[0x30, 0x59, 0x13], "wsr.mmid a3"),
    (&[0x30, 0x60, 0x03], "rsr.ibreakenable a3"),
    (&[0x30, 0x60, 0x13], "wsr.ibreakenable a3"),
    (&[0x30, 0x60, 0x61], "xsr.ibreakenable a3"),
    (&[0x30, 0x61, 0x03], "rsr.memctl a3"),
    (&[0x30, 0x61, 0x13], "wsr.memctl a3"),
    (&[0x30, 0x61, 0x61], "xsr.memctl a3"),
    (&[0x30, 0x63, 0x03], "rsr.atomctl a3"),
    (&[0x30, 0x63, 0x13], "wsr.atomctl a3"),
    (&[0x30, 0x63, 0x61], "xsr.atomctl a3"),
    (&[0x30, 0x68, 0x03], "rsr.ddr a3"),
    (&[0x30, 0x68, 0x13], "wsr.ddr a3"),
    (&[0x30, 0x68, 0x61], "xsr.ddr a3"),
    (&[0x30, 0x80, 0x03], "rsr.ibreaka0 a3"),
    (&[0x30, 0x80, 0x13], "wsr.ibreaka0 a3"),
    (&[0x30, 0x80, 0x61], "xsr.ibreaka0 a3"),
    (&[0x30, 0x81, 0x03], "rsr.ibreaka1 a3"),
    (&[0x30, 0x81, 0x13], "wsr.ibreaka1 a3"),
    (&[0x30, 0x81, 0x61], "xsr.ibreaka1 a3"),
    (&[0x30, 0x90, 0x03], "rsr.dbreaka0 a3"),
    (&[0x30, 0x90, 0x13], "wsr.dbreaka0 a3"),
    (&[0x30, 0x90, 0x61], "xsr.dbreaka0 a3"),
    (&[0x30, 0x91, 0x03], "rsr.dbreaka1 a3"),
    (&[0x30, 0x91, 0x13], "wsr.dbreaka1 a3"),
    (&[0x30, 0x91, 0x61], "xsr.dbreaka1 a3"),
    (&[0x30, 0xa0, 0x03], "rsr.dbreakc0 a3"),
    (&[0x30, 0xa0, 0x13], "wsr.dbreakc0 a3"),
    (&[0x30, 0xa0, 0x61], "xsr.dbreakc0 a3"),
    (&[0x30, 0xa1, 0x03], "rsr.dbreakc1 a3"),
    (&[0x30, 0xa1, 0x13], "wsr.dbreakc1 a3"),
    (&[0x30, 0xa1, 0x61], "xsr.dbreakc1 a3"),
    (&[0x30, 0xb1, 0x03], "rsr.epc1 a3"),
    (&[0x30, 0xb1, 0x13], "wsr.epc1 a3"),
    (&[0x30, 0xb1, 0x61], "xsr.epc1 a3"),
    (&[0x30, 0xb2, 0x03], "rsr.epc2 a3"),
    (&[0x30, 0xb2, 0x13], "wsr.epc2 a3"),
    (&[0x30, 0xb2, 0x61], "xsr.epc2 a3"),
    (&[0x30, 0xb3, 0x03], "rsr.epc3 a3"),
    (&[0x30, 0xb3, 0x13], "wsr.epc3 a3"),
    (&[0x30, 0xb3, 0x61], "xsr.epc3 a3"),
    (&[0x30, 0xb4, 0x03], "rsr.epc4 a3"),
    (&[0x30, 0xb4, 0x13], "wsr.epc4 a3"),
    (&[0x30, 0xb4, 0x61], "xsr.epc4 a3"),
    (&[0x30, 0xb5, 0x03], "rsr.epc5 a3"),
    (&[0x30, 0xb5, 0x13], "wsr.epc5 a3"),
    (&[0x30, 0xb5, 0x61], "xsr.epc5 a3"),
    (&[0x30, 0xb6, 0x03], "rsr.epc6 a3"),
    (&[0x30, 0xb6, 0x13], "wsr.epc6 a3"),
    (&[0x30, 0xb6, 0x61], "xsr.epc6 a3"),
    (&[0x30, 0xb7, 0x03], "rsr.epc7 a3"),
    (&[0x30, 0xb7, 0x13], "wsr.epc7 a3"),
    (&[0x30, 0xb7, 0x61], "xsr.epc7 a3"),
    (&[0x30, 0xc0, 0x03], "rsr.depc a3"),
    (&[0x30, 0xc0, 0x13], "wsr.depc a3"),
    (&[0x30, 0xc0, 0x61], "xsr.depc a3"),
    (&[0x30, 0xc2, 0x03], "rsr.eps2 a3"),
    (&[0x30, 0xc2, 0x13], "wsr.eps2 a3"),
    (&[0x30, 0xc2, 0x61], "xsr.eps2 a3"),
    (&[0x30, 0xc3, 0x03], "rsr.eps3 a3"),
    (&[0x30, 0xc3, 0x13], "wsr.eps3 a3"),
    (&[0x30, 0xc3, 0x61], "xsr.eps3 a3"),
    (&[0x30, 0xc4, 0x03], "rsr.eps4 a3"),
    (&[0x30, 0xc4, 0x13], "wsr.eps4 a3"),
    (&[0x30, 0xc4, 0x61], "xsr.eps4 a3"),
    (&[0x30, 0xc5, 0x03], "rsr.eps5 a3"),
    (&[0x30, 0xc5, 0x13], "wsr.eps5 a3"),
    (&[0x30, 0xc5, 0x61], "xsr.eps5 a3"),
    (&[0x30, 0xc6, 0x03], "rsr.eps6 a3"),
    (&[0x30, 0xc6, 0x13], "wsr.eps6 a3"),
    (&[0x30, 0xc6, 0x61], "xsr.eps6 a3"),
    (&[0x30, 0xc7, 0x03], "rsr.eps7 a3"),
    (&[0x30, 0xc7, 0x13], "wsr.eps7 a3"),
    (&[0x30, 0xc7, 0x61], "xsr.eps7 a3"),
    (&[0x30, 0xd1, 0x03], "rsr.excsave1 a3"),
    (&[0x30, 0xd1, 0x13], "wsr.excsave1 a3"),
    (&[0x30, 0xd1, 0x61], "xsr.excsave1 a3"),
    (&[0x30, 0xd2, 0x03], "rsr.excsave2 a3"),
    (&[0x30, 0xd2, 0x13], "wsr.excsave2 a3"),
    (&[0x30, 0xd2, 0x61], "xsr.excsave2 a3"),
    (&[0x30, 0xd3, 0x03], "rsr.excsave3 a3"),
    (&[0x30, 0xd3, 0x13], "wsr.excsave3 a3"),
    (&[0x30, 0xd3, 0x61], "xsr.excsave3 a3"),
    (&[0x30, 0xd4, 0x03], "rsr.excsave4 a3"),
    (&[0x30, 0xd4, 0x13], "wsr.excsave4 a3"),
    (&[0x30, 0xd4, 0x61], "xsr.excsave4 a3"),
    (&[0x30, 0xd5, 0x03], "rsr.excsave5 a3"),
    (&[0x30, 0xd5, 0x13], "wsr.excsave5 a3"),
    (&[0x30, 0xd5, 0x61], "xsr.excsave5 a3"),
    (&[0x30, 0xd6, 0x03], "rsr.excsave6 a3"),
    (&[0x30, 0xd6, 0x13], "wsr.excsave6 a3"),
    (&[0x30, 0xd6, 0x61], "xsr.excsave6 a3"),
    (&[0x30, 0xd7, 0x03], "rsr.excsave7 a3"),
    (&[0x30, 0xd7, 0x13], "wsr.excsave7 a3"),
    (&[0x30, 0xd7, 0x61], "xsr.excsave7 a3"),
    (&[0x30, 0xe0, 0x03], "rsr.cpenable a3"),
    (&[0x30, 0xe0, 0x13], "wsr.cpenable a3"),
    (&[0x30, 0xe0, 0x61], "xsr.cpenable a3"),
    (&[0x30, 0xe2, 0x03], "rsr.interrupt a3"),
    (&[0x30, 0xe2, 0x13], "wsr.intset a3"),
    (&[0x30, 0xe3, 0x13], "wsr.intclear a3"),
    (&[0x30, 0xe4, 0x03], "rsr.intenable a3"),
    (&[0x30, 0xe4, 0x13], "wsr.intenable a3"),
    (&[0x30, 0xe4, 0x61], "xsr.intenable a3"),
    (&[0x30, 0xe6, 0x03], "rsr.ps a3"),
    (&[0x30, 0xe6, 0x13], "wsr.ps a3"),
    (&[0x30, 0xe6, 0x61], "xsr.ps a3"),
    (&[0x30, 0xe7, 0x03], "rsr.vecbase a3"),
    (&[0x30, 0xe7, 0x13], "wsr.vecbase a3"),
    (&[0x30, 0xe7, 0x61], "xsr.vecbase a3"),
    (&[0x30, 0xe8, 0x03], "rsr.exccause a3"),
    (&[0x30, 0xe8, 0x13], "wsr.exccause a3"),
    (&[0x30, 0xe8, 0x61], "xsr.exccause a3"),
    (&[0x30, 0xe9, 0x03], "rsr.debugcause a3"),
    (&[0x30, 0xe9, 0x13], "wsr.debugcause a3"),
    (&[0x30, 0xe9, 0x61], "xsr.debugcause a3"),
    (&[0x30, 0xea, 0x03], "rsr.ccount a3"),
    (&[0x30, 0xea, 0x13], "wsr.ccount a3"),
    (&[0x30, 0xea, 0x61], "xsr.ccount a3"),
    (&[0x30, 0xeb, 0x03], "rsr.prid a3"),
    (&[0x30, 0xec, 0x03], "rsr.icount a3"),
    (&[0x30, 0xec, 0x13], "wsr.icount a3"),
    (&[0x30, 0xec, 0x61], "xsr.icount a3"),
    (&[0x30, 0xed, 0x03], "rsr.icountlevel a3"),
    (&[0x30, 0xed, 0x13], "wsr.icountlevel a3"),
    (&[0x30, 0xed, 0x61], "xsr.icountlevel a3"),
    (&[0x30, 0xee, 0x03], "rsr.excvaddr a3"),
    (&[0x30, 0xee, 0x13], "wsr.excvaddr a3"),
    (&[0x30, 0xee, 0x61], "xsr.excvaddr a3"),
    (&[0x30, 0xf0, 0x03], "rsr.ccompare0 a3"),
    (&[0x30, 0xf0, 0x13], "wsr.ccompare0 a3"),
    (&[0x30, 0xf0, 0x61], "xsr.ccompare0 a3"),
    (&[0x30, 0xf1, 0x03], "rsr.ccompare1 a3"),
    (&[0x30, 0xf1, 0x13], "wsr.ccompare1 a3"),
    (&[0x30, 0xf1, 0x61], "xsr.ccompare1 a3"),
    (&[0x30, 0xf2, 0x03], "rsr.ccompare2 a3"),
    (&[0x30, 0xf2, 0x13], "wsr.ccompare2 a3"),
    (&[0x30, 0xf2, 0x61], "xsr.ccompare2 a3"),
    (&[0x30, 0xf4, 0x03], "rsr.misc0 a3"),
    (&[0x30, 0xf4, 0x13], "wsr.misc0 a3"),
    (&[0x30, 0xf4, 0x61], "xsr.misc0 a3"),
    (&[0x30, 0xf5, 0x03], "rsr.misc1 a3"),
    (&[0x30, 0xf5, 0x13], "wsr.misc1 a3"),
    (&[0x30, 0xf5, 0x61], "xsr.misc1 a3"),
    (&[0x30, 0xf6, 0x03], "rsr.misc2 a3"),
    (&[0x30, 0xf6, 0x13], "wsr.misc2 a3"),
    (&[0x30, 0xf6, 0x61], "xsr.misc2 a3"),
    (&[0x30, 0xf7, 0x03], "rsr.misc3 a3"),
    (&[0x30, 0xf7, 0x13], "wsr.misc3 a3"),
    (&[0x30, 0xf7, 0x61], "xsr.misc3 a3"),
    (&[0x70, 0x3e, 0xe3], "rur.threadptr a3"),
    (&[0x30, 0xe7, 0xf3], "wur.threadptr a3"),
    (&[0x80, 0x3e, 0xe3], "rur.fcr a3"),
    (&[0x30, 0xe8, 0xf3], "wur.fcr a3"),
    (&[0x90, 0x3e, 0xe3], "rur.fsr a3"),
    (&[0x30, 0xe9, 0xf3], "wur.fsr a3"),
    (&[0xa0, 0x3e, 0xe3], "rur.f64r_lo a3"),
    (&[0x30, 0xea, 0xf3], "wur.f64r_lo a3"),
    (&[0xb0, 0x3e, 0xe3], "rur.f64r_hi a3"),
    (&[0x30, 0xeb, 0xf3], "wur.f64r_hi a3"),
    (&[0xc0, 0x3e, 0xe3], "rur.f64s a3"),
    (&[0x30, 0xec, 0xf3], "wur.f64s a3"),
    (&[0x60, 0x3e, 0xe3], "rur.expstate a3"),
    (&[0x30, 0xe6, 0xf3], "wur.expstate a3"),
];

/// Every legal `rsr`/`wsr`/`xsr`/`rur`/`wur` mnemonic the LX6 and LX7
/// assemblers accept decodes, encodes back to the same bytes, and
/// disassembles to objdump's own text.
#[test]
fn sr_ur_space() {
    assert_eq!(SR_UR_VECTORS.len(), 208, "the generated table lost entries");
    for (bytes, text) in SR_UR_VECTORS {
        dec(bytes);
        agrees_with_objdump(bytes, text);
    }
}

/// The vectors the phase file names by hand, checked structurally rather than
/// by string, so an operand-position mistake cannot hide behind a matching
/// mnemonic.
#[test]
fn sr_ur_named_vectors() {
    assert_eq!(
        dec(&[0x30, 0xe6, 0x03]),
        Inst::Sr(SrOp::Rsr, SpecialReg::Ps, a(3))
    );
    assert_eq!(
        dec(&[0x30, 0xe6, 0x13]),
        Inst::Sr(SrOp::Wsr, SpecialReg::Ps, a(3))
    );
    assert_eq!(
        dec(&[0x30, 0xe6, 0x61]),
        Inst::Sr(SrOp::Xsr, SpecialReg::Ps, a(3))
    );
    // `rur.threadptr a2` — the exact operand esp-rtos's task switch uses.
    assert_eq!(
        dec(&[0x70, 0x2e, 0xe3]),
        Inst::Ur(UrOp::Rur, UserReg::Threadptr, a(2))
    );
    assert_eq!(
        dec(&[0x30, 0xeb, 0x03]),
        Inst::Sr(SrOp::Rsr, SpecialReg::Prid, a(3))
    );
    // SCOMPARE1 must exist in the same commit as `s32c1i`.
    assert_eq!(
        dec(&[0x30, 0x0c, 0x13]),
        Inst::Sr(SrOp::Wsr, SpecialReg::Scompare1, a(3))
    );
}

// ---------------------------------------------------------------------------
// Family 1 — synchronising loads/stores and the store-conditional
// ---------------------------------------------------------------------------

/// Source lines, LX6 bytes == LX7 bytes for all of them:
/// ```text
///   l32ai a3, a4, 0        l32ai a3, a4, 4        l32ai a15, a0, 1020
///   s32ri a3, a4, 0        s32ri a3, a4, 4        s32ri a15, a0, 1020
///   s32c1i a2, a3, 4       s32c1i a0, a15, 0      s32c1i a15, a0, 1020
/// ```
#[rustfmt::skip]
const ATOMIC_VECTORS: &[(&[u8], &str)] = &[
    (&[0x32, 0xb4, 0x00], "l32ai a3, a4, 0"),
    (&[0x32, 0xb4, 0x01], "l32ai a3, a4, 4"),
    (&[0xf2, 0xb0, 0xff], "l32ai a15, a0, 0x3fc"),
    (&[0x32, 0xf4, 0x00], "s32ri a3, a4, 0"),
    (&[0x32, 0xf4, 0x01], "s32ri a3, a4, 4"),
    (&[0xf2, 0xf0, 0xff], "s32ri a15, a0, 0x3fc"),
    (&[0x22, 0xe3, 0x01], "s32c1i a2, a3, 4"),
    (&[0x02, 0xef, 0x00], "s32c1i a0, a15, 0"),
    (&[0xf2, 0xe0, 0xff], "s32c1i a15, a0, 0x3fc"),
];

#[test]
fn atomic_loads_and_stores() {
    for (bytes, text) in ATOMIC_VECTORS {
        dec(bytes);
        agrees_with_objdump(bytes, text);
    }
    // The phase file's named vector, checked structurally.
    assert_eq!(
        dec(&[0x22, 0xe3, 0x01]),
        Inst::AtomicLs(AtomicLsOp::S32c1i, a(2), a(3), 4)
    );
    assert_eq!(
        dec(&[0x32, 0xb4, 0x01]),
        Inst::AtomicLs(AtomicLsOp::L32ai, a(3), a(4), 4)
    );
    assert_eq!(
        dec(&[0xf2, 0xf0, 0xff]),
        Inst::AtomicLs(AtomicLsOp::S32ri, a(15), a(0), 1020)
    );
}

// ---------------------------------------------------------------------------
// Family 2 — zero-overhead loops
// ---------------------------------------------------------------------------

/// Assembled as one `.S` file so the offsets are real ones the assembler
/// computed, not values invented here. LX6 bytes == LX7 bytes.
///
/// ```text
///        loop a3, 1f      ; nop ; nop
///   1:   loopnez a3, 2f   ; nop
///   2:   loopgtz a4, 3f   ; memw ; memw ; memw
///   3:   loop a0, 4f      ; .rept 84 nop .endr
///   4:   loopnez a15, 5f  ; .rept 85 nop .endr
///   5:   nop
/// ```
///
/// objdump's listing, with the pc each instruction sat at:
///
/// | pc | bytes | text |
/// |---|---|---|
/// | 0x00 | `76 83 05` | `loop a3, 0x9` |
/// | 0x09 | `76 93 02` | `loopnez a3, 0xf` |
/// | 0x0f | `76 a4 08` | `loopgtz a4, 0x1b` |
/// | 0x1b | `76 80 fb` | `loop a0, 0x11a` |
/// | 0x11a | `76 9f fe` | `loopnez a15, 0x21c` |
#[rustfmt::skip]
const LOOP_VECTORS: &[(u32, &[u8], &str)] = &[
    (0x00,  &[0x76, 0x83, 0x05], "loop a3, 0x9"),
    (0x09,  &[0x76, 0x93, 0x02], "loopnez a3, 0xf"),
    (0x0f,  &[0x76, 0xa4, 0x08], "loopgtz a4, 0x1b"),
    (0x1b,  &[0x76, 0x80, 0xfb], "loop a0, 0x11a"),
    (0x11a, &[0x76, 0x9f, 0xfe], "loopnez a15, 0x21c"),
];

#[test]
fn zero_overhead_loops() {
    for (pc, bytes, text) in LOOP_VECTORS {
        dec(bytes);
        let mine = format_instruction(bytes, *pc);
        let (my_mnem, my_ops) = split(&mine);
        let (od_mnem, od_ops) = split(text);
        assert_eq!(my_mnem, od_mnem, "{mine} vs {text}");
        assert_eq!(my_ops, od_ops, "{mine} vs {text} at pc {pc:#x}");
    }

    assert_eq!(
        dec(&[0x76, 0x93, 0x02]),
        Inst::Loop(LoopOp::Loopnez, a(3), 2)
    );
    assert_eq!(dec(&[0x76, 0x83, 0x05]), Inst::Loop(LoopOp::Loop, a(3), 5));
    assert_eq!(
        dec(&[0x76, 0xa4, 0x08]),
        Inst::Loop(LoopOp::Loopgtz, a(4), 8)
    );
}

/// `LEND = pc + 4 + imm8`, checked against the address objdump printed for the
/// same bytes at the same pc — including the two large offsets, where reading
/// the field as *signed* would land 256 bytes backwards instead.
#[test]
fn loop_end_matches_objdump() {
    for (pc, bytes, text) in LOOP_VECTORS {
        let Inst::Loop(_, _, imm) = decode(bytes).unwrap().0 else {
            panic!("not a loop: {bytes:02x?}");
        };
        let expected = parse_int(split(text).1[1].as_str()).unwrap() as u32;
        assert_eq!(
            lp_xt_inst::disasm::loop_end(*pc, imm),
            expected,
            "LEND for {text} at pc {pc:#x}"
        );
    }
    // The two big ones read forward, not backward.
    assert_eq!(lp_xt_inst::disasm::loop_end(0x1b, 0xfb), 0x11a);
    assert_eq!(lp_xt_inst::disasm::loop_end(0x11a, 0xfe), 0x21c);
}

// ---------------------------------------------------------------------------
// Families 4 and 5 — privileged control flow, windows, traps
// ---------------------------------------------------------------------------
//
// Committed together because they share one opcode neighbourhood (`op0 = 0`,
// `op1 = 0`, `op2 = 0`, sub-selected by `r`: 3 = the exception returns,
// 4 = `break`, 5 = `syscall`, 6 = `rsil`, 7 = `waiti`) and came out of one
// assembler probe.

/// LX6 bytes == LX7 bytes for every vector.
#[rustfmt::skip]
const PRIV_VECTORS: &[(&[u8], &str)] = &[
    (&[0x20, 0x63, 0x00], "rsil a2, 3"),
    (&[0x00, 0x60, 0x00], "rsil a0, 0"),
    (&[0xf0, 0x6f, 0x00], "rsil a15, 15"),
    (&[0x00, 0x70, 0x00], "waiti 0"),
    (&[0x00, 0x7f, 0x00], "waiti 15"),
    (&[0x00, 0x30, 0x00], "rfe"),
    (&[0x00, 0x32, 0x00], "rfde"),
    (&[0x10, 0x33, 0x00], "rfi 3"),
    (&[0x10, 0x30, 0x00], "rfi 0"),
    (&[0x10, 0x3f, 0x00], "rfi 15"),
    (&[0x00, 0x34, 0x00], "rfwo"),
    (&[0x00, 0x35, 0x00], "rfwu"),
    (&[0xf0, 0x80, 0x40], "rotw -1"),
    (&[0x10, 0x80, 0x40], "rotw 1"),
    (&[0x80, 0x80, 0x40], "rotw -8"),
    (&[0x70, 0x80, 0x40], "rotw 7"),
    (&[0x00, 0xc5, 0x09], "l32e a0, a5, -16"),
    (&[0xf0, 0x00, 0x09], "l32e a15, a0, -64"),
    (&[0x00, 0xc5, 0x49], "s32e a0, a5, -16"),
    (&[0xf0, 0x00, 0x49], "s32e a15, a0, -64"),
    (&[0x10, 0x41, 0x00], "break 1, 1"),
    (&[0x00, 0x40, 0x00], "break 0, 0"),
    (&[0xf0, 0x4f, 0x00], "break 15, 15"),
    (&[0x20, 0x41, 0x00], "break 1, 2"),
    (&[0x10, 0x42, 0x00], "break 2, 1"),
    (&[0x40, 0x43, 0x00], "break 3, 4"),
    (&[0x2d, 0xf1], "break.n 1"),
    (&[0x2d, 0xff], "break.n 15"),
    (&[0x00, 0x50, 0x00], "syscall"),
    // Already decoded before this phase; the phase file asks for the vector.
    (&[0x10, 0x13, 0x00], "movsp a1, a3"),
    (&[0x10, 0x10, 0x00], "movsp a1, a0"),
];

#[test]
fn privileged_control_flow_and_windows() {
    for (bytes, text) in PRIV_VECTORS {
        dec(bytes);
        agrees_with_objdump(bytes, text);
    }

    // The exact operands the real `_WindowOverflow4` / `_WindowUnderflow4`
    // handlers use (`xtensa-lx-rt-0.22.0/src/exception/asm.rs:786-791`).
    assert_eq!(
        dec(&[0x00, 0xc5, 0x09]),
        Inst::WindowLs(WindowLsOp::L32e, a(0), a(5), -16)
    );
    assert_eq!(
        dec(&[0x00, 0xc5, 0x49]),
        Inst::WindowLs(WindowLsOp::S32e, a(0), a(5), -16)
    );
    assert_eq!(dec(&[0x00, 0x34, 0x00]), Inst::Rf(RfOp::Rfwo));
    assert_eq!(dec(&[0x00, 0x35, 0x00]), Inst::Rf(RfOp::Rfwu));
    assert_eq!(dec(&[0x00, 0x30, 0x00]), Inst::Rf(RfOp::Rfe));
    assert_eq!(dec(&[0x10, 0x33, 0x00]), Inst::Rfi(3));
    assert_eq!(dec(&[0xf0, 0x80, 0x40]), Inst::Rotw(-1));
    assert_eq!(dec(&[0x20, 0x63, 0x00]), Inst::Rsil(a(2), 3));
    assert_eq!(dec(&[0x00, 0x70, 0x00]), Inst::Waiti(0));
    // `break imms, immt` — `s` is the FIRST operand, `t` the second. The
    // asymmetric vectors are what pin the order.
    assert_eq!(dec(&[0x20, 0x41, 0x00]), Inst::Break(1, 2));
    assert_eq!(dec(&[0x10, 0x42, 0x00]), Inst::Break(2, 1));
    assert_eq!(dec(&[0x2d, 0xf1]), Inst::BreakN(1));
}

// ---------------------------------------------------------------------------
// Families 6, 7, 8 — the integer helper, the boolean file, region protection
// ---------------------------------------------------------------------------

/// LX6 bytes == LX7 bytes for every vector, `rer`/`wer` included.
///
/// objdump renders the boolean reductions with the whole register range
/// (`all4 b0, b4:b5:b6:b7`); the field holds only the first register of the
/// group, which is what `agrees_with_objdump` compares.
#[rustfmt::skip]
const HELPER_VECTORS: &[(&[u8], &str)] = &[
    (&[0x00, 0x34, 0x33], "clamps a3, a4, 7"),
    (&[0xf0, 0x34, 0x33], "clamps a3, a4, 22"),
    (&[0x20, 0x01, 0x02], "andb b0, b1, b2"),
    (&[0x20, 0x01, 0x12], "andbc b0, b1, b2"),
    (&[0x20, 0x01, 0x22], "orb b0, b1, b2"),
    (&[0x20, 0x01, 0x32], "orbc b0, b1, b2"),
    (&[0x20, 0x01, 0x42], "xorb b0, b1, b2"),
    (&[0x00, 0x94, 0x00], "all4 b0, b4:b5:b6:b7"),
    (&[0x00, 0x84, 0x00], "any4 b0, b4:b5:b6:b7"),
    (&[0x00, 0xb8, 0x00], "all8 b0, b8:b9:b10:b11:b12:b13:b14:b15"),
    (&[0x00, 0xa8, 0x00], "any8 b0, b8:b9:b10:b11:b12:b13:b14:b15"),
    (&[0x00, 0xc3, 0x50], "idtlb a3"),
    (&[0x00, 0x43, 0x50], "iitlb a3"),
    (&[0x20, 0xe3, 0x50], "wdtlb a2, a3"),
    (&[0x20, 0x63, 0x50], "witlb a2, a3"),
    (&[0x20, 0xb3, 0x50], "rdtlb0 a2, a3"),
    (&[0x20, 0xf3, 0x50], "rdtlb1 a2, a3"),
    (&[0x20, 0x33, 0x50], "ritlb0 a2, a3"),
    (&[0x20, 0x73, 0x50], "ritlb1 a2, a3"),
    (&[0x20, 0xd3, 0x50], "pdtlb a2, a3"),
    (&[0x20, 0x53, 0x50], "pitlb a2, a3"),
    (&[0x40, 0x60, 0x40], "rer a4, a0"),
    (&[0x40, 0x70, 0x40], "wer a4, a0"),
    (&[0xf0, 0x63, 0x40], "rer a15, a3"),
    (&[0x00, 0x7f, 0x40], "wer a0, a15"),
];

#[test]
fn integer_helper_booleans_and_region_protection() {
    for (bytes, text) in HELPER_VECTORS {
        dec(bytes);
        agrees_with_objdump(bytes, text);
    }
    assert_eq!(dec(&[0x00, 0x34, 0x33]), Inst::Clamps(a(3), a(4), 7));
    assert_eq!(
        dec(&[0x20, 0x01, 0x42]),
        Inst::BoolLogic(BoolOp::Xorb, br(0), br(1), br(2))
    );
    assert_eq!(
        dec(&[0x00, 0x94, 0x00]),
        Inst::BoolAll(BoolAllOp::All4, br(0), br(4))
    );
    assert_eq!(
        dec(&[0x20, 0xe3, 0x50]),
        Inst::Tlb(TlbOp::Wdtlb, a(2), a(3))
    );
    assert_eq!(dec(&[0x00, 0xc3, 0x50]), Inst::TlbInv(true, a(3)));
    assert_eq!(dec(&[0x00, 0x43, 0x50]), Inst::TlbInv(false, a(3)));
    assert_eq!(dec(&[0x40, 0x70, 0x40]), Inst::ExtReg(true, a(4), a(0)));
}

// ---------------------------------------------------------------------------
// Family 9 — MAC16
// ---------------------------------------------------------------------------

/// The whole MAC16 space the LX6 and LX7 assemblers accept, one vector per
/// legal mnemonic: `umul`/`mul`/`mula`/`muls` x `aa`/`ad`/`da`/`dd` x the four
/// half selectors, the eight `mula.*.ldinc`/`.lddec` forms, and `ldinc`/`lddec`
/// for all four `mw`. LX6 bytes == LX7 bytes throughout.
///
/// Two facts the table pins that a from-first-principles guess would miss:
/// the **x** MR operand is one bit (`m0`/`m1`, in `r{3-2}`) and the **y** MR
/// operand is one bit (`m2`/`m3`, in `t{2}`) — assembling `mul.da.ll m2, a3`
/// produces the same word as `m0` and disassembles back as `m0`.
#[rustfmt::skip]
const MAC16_VECTORS: &[(&[u8], &str)] = &[
    (&[0x34, 0x02, 0x74], "mul.aa.ll a2, a3"),
    (&[0x04, 0x02, 0x34], "mul.ad.ll a2, m2"),
    (&[0x34, 0x40, 0x64], "mul.da.ll m1, a3"),
    (&[0x04, 0x40, 0x24], "mul.dd.ll m1, m2"),
    (&[0x34, 0x02, 0x78], "mula.aa.ll a2, a3"),
    (&[0x04, 0x02, 0x38], "mula.ad.ll a2, m2"),
    (&[0x34, 0x40, 0x68], "mula.da.ll m1, a3"),
    (&[0x04, 0x40, 0x28], "mula.dd.ll m1, m2"),
    (&[0x34, 0x02, 0x7c], "muls.aa.ll a2, a3"),
    (&[0x04, 0x02, 0x3c], "muls.ad.ll a2, m2"),
    (&[0x34, 0x40, 0x6c], "muls.da.ll m1, a3"),
    (&[0x04, 0x40, 0x2c], "muls.dd.ll m1, m2"),
    (&[0x34, 0x02, 0x70], "umul.aa.ll a2, a3"),
    (&[0x34, 0x48, 0x48], "mula.da.ll.ldinc m0, a8, m1, a3"),
    (&[0x04, 0x48, 0x08], "mula.dd.ll.ldinc m0, a8, m1, m2"),
    (&[0x34, 0x48, 0x58], "mula.da.ll.lddec m0, a8, m1, a3"),
    (&[0x04, 0x48, 0x18], "mula.dd.ll.lddec m0, a8, m1, m2"),
    (&[0x34, 0x02, 0x75], "mul.aa.hl a2, a3"),
    (&[0x04, 0x02, 0x35], "mul.ad.hl a2, m2"),
    (&[0x34, 0x40, 0x65], "mul.da.hl m1, a3"),
    (&[0x04, 0x40, 0x25], "mul.dd.hl m1, m2"),
    (&[0x34, 0x02, 0x79], "mula.aa.hl a2, a3"),
    (&[0x04, 0x02, 0x39], "mula.ad.hl a2, m2"),
    (&[0x34, 0x40, 0x69], "mula.da.hl m1, a3"),
    (&[0x04, 0x40, 0x29], "mula.dd.hl m1, m2"),
    (&[0x34, 0x02, 0x7d], "muls.aa.hl a2, a3"),
    (&[0x04, 0x02, 0x3d], "muls.ad.hl a2, m2"),
    (&[0x34, 0x40, 0x6d], "muls.da.hl m1, a3"),
    (&[0x04, 0x40, 0x2d], "muls.dd.hl m1, m2"),
    (&[0x34, 0x02, 0x71], "umul.aa.hl a2, a3"),
    (&[0x34, 0x48, 0x49], "mula.da.hl.ldinc m0, a8, m1, a3"),
    (&[0x04, 0x48, 0x09], "mula.dd.hl.ldinc m0, a8, m1, m2"),
    (&[0x34, 0x48, 0x59], "mula.da.hl.lddec m0, a8, m1, a3"),
    (&[0x04, 0x48, 0x19], "mula.dd.hl.lddec m0, a8, m1, m2"),
    (&[0x34, 0x02, 0x76], "mul.aa.lh a2, a3"),
    (&[0x04, 0x02, 0x36], "mul.ad.lh a2, m2"),
    (&[0x34, 0x40, 0x66], "mul.da.lh m1, a3"),
    (&[0x04, 0x40, 0x26], "mul.dd.lh m1, m2"),
    (&[0x34, 0x02, 0x7a], "mula.aa.lh a2, a3"),
    (&[0x04, 0x02, 0x3a], "mula.ad.lh a2, m2"),
    (&[0x34, 0x40, 0x6a], "mula.da.lh m1, a3"),
    (&[0x04, 0x40, 0x2a], "mula.dd.lh m1, m2"),
    (&[0x34, 0x02, 0x7e], "muls.aa.lh a2, a3"),
    (&[0x04, 0x02, 0x3e], "muls.ad.lh a2, m2"),
    (&[0x34, 0x40, 0x6e], "muls.da.lh m1, a3"),
    (&[0x04, 0x40, 0x2e], "muls.dd.lh m1, m2"),
    (&[0x34, 0x02, 0x72], "umul.aa.lh a2, a3"),
    (&[0x34, 0x48, 0x4a], "mula.da.lh.ldinc m0, a8, m1, a3"),
    (&[0x04, 0x48, 0x0a], "mula.dd.lh.ldinc m0, a8, m1, m2"),
    (&[0x34, 0x48, 0x5a], "mula.da.lh.lddec m0, a8, m1, a3"),
    (&[0x04, 0x48, 0x1a], "mula.dd.lh.lddec m0, a8, m1, m2"),
    (&[0x34, 0x02, 0x77], "mul.aa.hh a2, a3"),
    (&[0x04, 0x02, 0x37], "mul.ad.hh a2, m2"),
    (&[0x34, 0x40, 0x67], "mul.da.hh m1, a3"),
    (&[0x04, 0x40, 0x27], "mul.dd.hh m1, m2"),
    (&[0x34, 0x02, 0x7b], "mula.aa.hh a2, a3"),
    (&[0x04, 0x02, 0x3b], "mula.ad.hh a2, m2"),
    (&[0x34, 0x40, 0x6b], "mula.da.hh m1, a3"),
    (&[0x04, 0x40, 0x2b], "mula.dd.hh m1, m2"),
    (&[0x34, 0x02, 0x7f], "muls.aa.hh a2, a3"),
    (&[0x04, 0x02, 0x3f], "muls.ad.hh a2, m2"),
    (&[0x34, 0x40, 0x6f], "muls.da.hh m1, a3"),
    (&[0x04, 0x40, 0x2f], "muls.dd.hh m1, m2"),
    (&[0x34, 0x02, 0x73], "umul.aa.hh a2, a3"),
    (&[0x34, 0x48, 0x4b], "mula.da.hh.ldinc m0, a8, m1, a3"),
    (&[0x04, 0x48, 0x0b], "mula.dd.hh.ldinc m0, a8, m1, m2"),
    (&[0x34, 0x48, 0x5b], "mula.da.hh.lddec m0, a8, m1, a3"),
    (&[0x04, 0x48, 0x1b], "mula.dd.hh.lddec m0, a8, m1, m2"),
    (&[0x04, 0x08, 0x80], "ldinc m0, a8"),
    (&[0x04, 0x08, 0x90], "lddec m0, a8"),
    (&[0x04, 0x18, 0x80], "ldinc m1, a8"),
    (&[0x04, 0x18, 0x90], "lddec m1, a8"),
    (&[0x04, 0x28, 0x80], "ldinc m2, a8"),
    (&[0x04, 0x28, 0x90], "lddec m2, a8"),
    (&[0x04, 0x38, 0x80], "ldinc m3, a8"),
    (&[0x04, 0x38, 0x90], "lddec m3, a8"),
    (&[0x34, 0x00, 0x64], "mul.da.ll m0, a3"),
    (&[0x44, 0x02, 0x34], "mul.ad.ll a2, m3"),
];

#[test]
fn mac16() {
    assert_eq!(MAC16_VECTORS.len(), 78, "the generated table lost entries");
    for (bytes, text) in MAC16_VECTORS {
        dec(bytes);
        agrees_with_objdump(bytes, text);
    }

    assert_eq!(
        dec(&[0x34, 0x02, 0x74]),
        Inst::Mac(MacOp::Mul, MacHalf::Ll, MacSrc::Aa(a(2), a(3)))
    );
    assert_eq!(
        dec(&[0x44, 0x02, 0x34]),
        Inst::Mac(MacOp::Mul, MacHalf::Ll, MacSrc::Ad(a(2), m(3)))
    );
    assert_eq!(
        dec(&[0x34, 0x40, 0x64]),
        Inst::Mac(MacOp::Mul, MacHalf::Ll, MacSrc::Da(m(1), a(3)))
    );
    assert_eq!(
        dec(&[0x04, 0x40, 0x24]),
        Inst::Mac(MacOp::Mul, MacHalf::Ll, MacSrc::Dd(m(1), m(2)))
    );
    assert_eq!(
        dec(&[0x34, 0x02, 0x70]),
        Inst::Mac(MacOp::Umul, MacHalf::Ll, MacSrc::Aa(a(2), a(3)))
    );
    // The two forms M0's inventory found in the classic ROM and the bootloader.
    assert_eq!(
        dec(&[0x04, 0x48, 0x08]),
        Inst::MacLd(false, MacHalf::Ll, m(0), a(8), m(1), MacY::Mr(m(2)))
    );
    assert_eq!(
        dec(&[0x04, 0x48, 0x18]),
        Inst::MacLd(true, MacHalf::Ll, m(0), a(8), m(1), MacY::Mr(m(2)))
    );
    assert_eq!(dec(&[0x04, 0x38, 0x80]), Inst::MacLoad(false, m(3), a(8)));
    assert_eq!(dec(&[0x04, 0x38, 0x90]), Inst::MacLoad(true, m(3), a(8)));
}

/// `umul` exists only in the AA operand form, and MAC16 words with a reserved
/// field set are not MAC16 instructions. Neither is guessed at.
#[test]
fn mac16_reserved_fields_and_umul_are_not_guessed() {
    // op0 = 4, op1 = 0 (umul), op2 = 3/6/2 -> `umul.ad`/`.da`/`.dd`, which
    // neither assembler accepts.
    for op2 in [0x2u32, 0x3, 0x6] {
        let w = 4 | (op2 << 20);
        let bytes = [w as u8, (w >> 8) as u8, (w >> 16) as u8];
        assert!(
            decode(&bytes).is_err(),
            "umul has no {op2:#x} operand form: {bytes:02x?}"
        );
    }
    // `mul.aa.ll a2, a3` with `r` (reserved) set.
    assert!(decode(&[0x34, 0x12, 0x74]).is_err());
    // `mul.da.ll m1, a3` with `s` (reserved) set.
    assert!(decode(&[0x34, 0x41, 0x64]).is_err());
    // `mul.dd.ll m1, m2` with t{0} (reserved) set.
    assert!(decode(&[0x14, 0x40, 0x24]).is_err());
    // `ldinc m0, a8` with op1 (reserved) set.
    assert!(decode(&[0x04, 0x08, 0x81]).is_err());
}

// ---------------------------------------------------------------------------
// The SSAI reserved-field fix
// ---------------------------------------------------------------------------

/// `ssai` carries a 5-bit immediate in `s` plus `t{0}`; `t{3-1}` is reserved
/// and the assembler always emits it as zero.
///
/// Bytes `40 40 40` (word `0x404040`, `t = 4`) sit in a literal pool in the
/// shipped `fw-esp32v3` image at 0x400d223b. Before this phase they decoded as
/// `ssai 0` — a silently wrong answer. objdump *renders* them `lsi f4, a0,
/// 0x100`, but that is an LX6-only loose table entry, not a real instruction:
///
/// - `xtensa-esp32-elf-as` encodes `lsi f4, a0, 0x100` as `43 00 40`
///   (word `0x400043`, `op0 = 3`), never as `0x404040`;
/// - `xtensa-esp32s3-elf-objdump` calls `0x404040` undecodable;
/// - `xtensa-esp32-elf-objdump` renders **every** word with `op0 = 0` and an
///   unassigned `(op1, op2, r)` as `lsi` — `0x400040`, `0x401040`, `0x402040`,
///   `0x403040`, `0x405040`, `0x409040`… all disassemble as
///   `lsi f4, a0, 0x100`, with the same operands.
///
/// So the correct answer for these bytes is `Unsupported`, and that is what
/// this asserts.
#[test]
fn ssai_reserved_field_must_be_zero() {
    // The real thing still decodes: `ssai 0` and `ssai 31`.
    assert_eq!(dec(&[0x00, 0x40, 0x40]), Inst::Ssai(0));
    assert_eq!(dec(&[0x10, 0x4f, 0x40]), Inst::Ssai(31));

    // The literal-pool words do not. 0x404040 is the one M0's inventory found
    // in the image (six sites) and in the classic ROM (one site).
    for bytes in [
        [0x40u8, 0x40, 0x40], // t = 4
        [0x20, 0x40, 0x40],   // t = 2
        [0x50, 0x40, 0x40],   // t = 5
        [0xb0, 0x4a, 0x40],   // t = 0xb
    ] {
        assert!(
            matches!(
                decode(&bytes).unwrap_err(),
                DecodeError::Unsupported { len: 3, .. }
            ),
            "{bytes:02x?} has ssai's reserved t bits set and must not decode as ssai"
        );
    }
}
