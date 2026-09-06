//! The FP conformance replay: **no board required, ever**.
//!
//! This is the artifact that keeps the emulator honest after M6 ends. It runs
//! every vector of [`lp_xt_fp_vectors`]'s six families through the emulator and
//! compares the answer to what is committed under `tests/fixtures/fp/`.
//!
//! # Why the predictions were committed before hardware (D2)
//!
//! Silicon is the spec for this chip, and the obvious way to use that is: run
//! the vectors on the S3, capture the output, make the emulator match. That
//! produces an emulator that agrees with the device and **proves nothing** — a
//! mis-ordered result block, a kernel reading the wrong operand, or a generator
//! that drifted between the two sides is indistinguishable from success. It is
//! a tautology pointed the other way from the usual one.
//!
//! So: predictions first, committed, *then* hardware. A divergence then has
//! three readings — emulator bug, harness bug, or genuine silicon behavior — and
//! P6's job is to tell them apart. This is already the repo's rule twice over;
//! see `lpvm-native/src/xt_corpus.rs` and `just fwtest-xt-jit-esp32s3`.
//!
//! # `UNKNOWN` rows
//!
//! A row whose prediction needs an unresolved [`lp_xt_emu::FpPolicy`] field is
//! recorded as `UNKNOWN:<field>` — not a failure, a **question addressed to
//! silicon**, and P6 answers it. The set is *derived*: the harness runs the
//! vector, catches the policy panic, and reads the field name out of it, so the
//! UNKNOWN rows cannot drift away from what the executors actually need.
//!
//! A run whose unknown count is unexpectedly **zero** is as suspicious as one
//! where it is huge, so the count is printed and asserted non-zero.
//!
//! # Regenerating
//!
//! ```bash
//! UPDATE_FP_GOLDENS=1 cargo test -p lp-xt-emu --test fp_conformance
//! ```
//!
//! That regenerates from the **emulator**, which is what a prediction is.
//! **Never** regenerate one from device output: that is the tautology this file
//! exists to prevent, and it is the repo's stated rule.

use std::fmt::Write as _;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;

use lp_xt_emu::cpu::CPENABLE_FPU;
use lp_xt_emu::fp_capture::{Prediction, parse_predictions};
use lp_xt_emu::fp_policy::parse_unresolved;
use lp_xt_emu::{Emulator, Trap};
use lp_xt_fp_vectors::{Family, OpCode, Vector, count, fingerprint, vector};
use lp_xt_inst::{BReg, FReg, FpCmpOp, FpRrOp, FpRrrOp, FpToIntOp, Inst, IntToFpOp, Reg};

/// Where the operands and the result live while a vector runs.
const DEST_F: u8 = 0;
const SRC_A_F: u8 = 1;
const SRC_B_F: u8 = 2;
const DEST_B: u8 = 0;
const SRC_INT_A: u8 = 2;
const DEST_INT_A: u8 = 3;

/// The toolchain's divide sequence, `f0 = f1 / f2` — instruction for
/// instruction the body of the esp-14.2.0 libgcc `__divsf3` (transcribed from
/// objdump; output-as-fact, no library source read). The same text lives as a
/// `global_asm!` kernel in `fw-esp32s3`'s conformance harness, so the device
/// and the emulator run the *same* sequence and the 272 F5 rows compare like
/// for like.
fn div_sequence() -> Vec<Inst> {
    let f = FReg::new;
    vec![
        Inst::FpRr(FpRrOp::Div0S, f(3), f(2)),
        Inst::FpRr(FpRrOp::Nexp01S, f(4), f(2)),
        Inst::ConstS(f(5), 1),
        Inst::FpRrr(FpRrrOp::MaddnS, f(5), f(4), f(3)),
        Inst::FpRr(FpRrOp::MovS, f(6), f(3)),
        Inst::FpRr(FpRrOp::MovS, f(7), f(2)),
        Inst::FpRr(FpRrOp::Nexp01S, f(2), f(1)),
        Inst::FpRrr(FpRrrOp::MaddnS, f(6), f(5), f(6)),
        Inst::ConstS(f(5), 1),
        Inst::ConstS(f(0), 0),
        Inst::FpRr(FpRrOp::NegS, f(8), f(2)),
        Inst::FpRrr(FpRrrOp::MaddnS, f(5), f(4), f(6)),
        Inst::FpRrr(FpRrrOp::MaddnS, f(0), f(8), f(3)),
        Inst::FpRr(FpRrOp::MkdadjS, f(7), f(1)),
        Inst::FpRrr(FpRrrOp::MaddnS, f(6), f(5), f(6)),
        Inst::FpRrr(FpRrrOp::MaddnS, f(8), f(4), f(0)),
        Inst::ConstS(f(3), 1),
        Inst::FpRrr(FpRrrOp::MaddnS, f(3), f(4), f(6)),
        Inst::FpRrr(FpRrrOp::MaddnS, f(0), f(8), f(6)),
        Inst::FpRr(FpRrOp::NegS, f(2), f(2)),
        Inst::FpRrr(FpRrrOp::MaddnS, f(6), f(3), f(6)),
        Inst::FpRrr(FpRrrOp::MaddnS, f(2), f(4), f(0)),
        Inst::FpRr(FpRrOp::AddexpmS, f(0), f(7)),
        Inst::FpRr(FpRrOp::AddexpS, f(6), f(7)),
        Inst::FpRrr(FpRrrOp::DivnS, f(0), f(2), f(6)),
    ]
}

/// The toolchain's square-root sequence, `f0 = sqrt(f1)` — the body of the
/// esp-14.2.0 libm `__ieee754_sqrtf` (the raw sequence, not the errno-setting
/// wrapper). Same provenance and same device-side twin as [`div_sequence`].
fn sqrt_sequence() -> Vec<Inst> {
    let f = FReg::new;
    vec![
        Inst::FpRr(FpRrOp::Sqrt0S, f(2), f(1)),
        Inst::ConstS(f(3), 0),
        Inst::FpRrr(FpRrrOp::MaddnS, f(3), f(2), f(2)),
        Inst::FpRr(FpRrOp::Nexp01S, f(4), f(1)),
        Inst::ConstS(f(0), 3),
        Inst::FpRr(FpRrOp::AddexpS, f(4), f(0)),
        Inst::FpRrr(FpRrrOp::MaddnS, f(0), f(3), f(4)),
        Inst::FpRr(FpRrOp::Nexp01S, f(3), f(1)),
        Inst::FpRr(FpRrOp::NegS, f(5), f(3)),
        Inst::FpRrr(FpRrrOp::MaddnS, f(2), f(0), f(2)),
        Inst::ConstS(f(0), 0),
        Inst::ConstS(f(6), 0),
        Inst::ConstS(f(7), 0),
        Inst::FpRrr(FpRrrOp::MaddnS, f(0), f(5), f(2)),
        Inst::FpRrr(FpRrrOp::MaddnS, f(6), f(2), f(4)),
        Inst::ConstS(f(4), 3),
        Inst::FpRrr(FpRrrOp::MaddnS, f(7), f(4), f(2)),
        Inst::FpRrr(FpRrrOp::MaddnS, f(3), f(0), f(0)),
        Inst::FpRrr(FpRrrOp::MaddnS, f(4), f(6), f(2)),
        Inst::FpRr(FpRrOp::NegS, f(2), f(7)),
        Inst::FpRrr(FpRrrOp::MaddnS, f(0), f(3), f(2)),
        Inst::FpRrr(FpRrrOp::MaddnS, f(7), f(4), f(7)),
        Inst::FpRr(FpRrOp::MksadjS, f(2), f(1)),
        Inst::FpRr(FpRrOp::Nexp01S, f(1), f(1)),
        Inst::FpRrr(FpRrrOp::MaddnS, f(1), f(0), f(0)),
        Inst::FpRr(FpRrOp::NegS, f(3), f(7)),
        Inst::FpRr(FpRrOp::AddexpmS, f(0), f(2)),
        Inst::FpRr(FpRrOp::AddexpS, f(3), f(2)),
        Inst::FpRrr(FpRrrOp::DivnS, f(0), f(1), f(3)),
    ]
}

/// Build the instruction a vector names, or `None` for the pseudo-ops.
fn instruction(v: &Vector) -> Option<Inst> {
    let (d, a, b) = (FReg::new(DEST_F), FReg::new(SRC_A_F), FReg::new(SRC_B_F));
    let rrr = |op| Some(Inst::FpRrr(op, d, a, b));
    let rr = |op| Some(Inst::FpRr(op, d, a));
    let cmp = |op| Some(Inst::FpCmp(op, BReg::new(DEST_B), a, b));
    let to_int = |op| Some(Inst::FpToInt(op, Reg::new(DEST_INT_A), a, v.imm));
    match v.op {
        OpCode::AddS => rrr(FpRrrOp::AddS),
        OpCode::SubS => rrr(FpRrrOp::SubS),
        OpCode::MulS => rrr(FpRrrOp::MulS),
        // madd/msub accumulate into the destination, so `c` is staged there.
        OpCode::MaddS => rrr(FpRrrOp::MaddS),
        OpCode::MsubS => rrr(FpRrrOp::MsubS),
        OpCode::AbsS => rr(FpRrOp::AbsS),
        OpCode::NegS => rr(FpRrOp::NegS),
        OpCode::MovS => rr(FpRrOp::MovS),
        OpCode::Recip0S => rr(FpRrOp::Recip0S),
        OpCode::Rsqrt0S => rr(FpRrOp::Rsqrt0S),
        OpCode::Sqrt0S => rr(FpRrOp::Sqrt0S),
        OpCode::Div0S => rr(FpRrOp::Div0S),
        OpCode::OeqS => cmp(FpCmpOp::OeqS),
        OpCode::OltS => cmp(FpCmpOp::OltS),
        OpCode::OleS => cmp(FpCmpOp::OleS),
        OpCode::UeqS => cmp(FpCmpOp::UeqS),
        OpCode::UltS => cmp(FpCmpOp::UltS),
        OpCode::UleS => cmp(FpCmpOp::UleS),
        OpCode::UnS => cmp(FpCmpOp::UnS),
        OpCode::TruncS => to_int(FpToIntOp::TruncS),
        OpCode::UtruncS => to_int(FpToIntOp::UtruncS),
        OpCode::RoundS => to_int(FpToIntOp::RoundS),
        OpCode::FloorS => to_int(FpToIntOp::FloorS),
        OpCode::CeilS => to_int(FpToIntOp::CeilS),
        OpCode::FloatS => Some(Inst::IntToFp(
            IntToFpOp::FloatS,
            d,
            Reg::new(SRC_INT_A),
            v.imm,
        )),
        OpCode::UfloatS => Some(Inst::IntToFp(
            IntToFpOp::UfloatS,
            d,
            Reg::new(SRC_INT_A),
            v.imm,
        )),
        // Divide and square root are code sequences, not instructions.
        OpCode::Div | OpCode::Sqrt => None,
    }
}

/// Run one vector on a fresh-state emulator: `(result, FSR)` predictions.
///
/// The FSR column is a first-class prediction since the P6 campaign resolved
/// the flag semantics: the register is cleared before the vector and read
/// after it, exactly as the device harness does.
fn predict(emu: &mut Emulator, v: &Vector) -> (Prediction, Prediction) {
    // Reset only what a vector touches; the memory map is expensive to rebuild
    // and no vector reads it.
    emu.cpu.fr = [0; 16];
    emu.cpu.br = 0;
    emu.cpu.fsr = 0;
    emu.cpu.fcr = u32::from(v.fcr);
    emu.cpu.cpenable = CPENABLE_FPU;
    emu.cpu.set_f(SRC_A_F, v.a);
    emu.cpu.set_f(SRC_B_F, v.b);
    emu.cpu.set_f(DEST_F, v.c);
    emu.cpu.set_a(SRC_INT_A, v.a);
    emu.cpu.set_a(DEST_INT_A, 0);

    let insts: Vec<Inst> = match instruction(v) {
        Some(inst) => vec![inst],
        // The toolchain's divide and square-root sequences, running on the
        // measured helper semantics. The device runs the same sequences.
        None => match v.op {
            OpCode::Div => div_sequence(),
            OpCode::Sqrt => sqrt_sequence(),
            _ => unreachable!(),
        },
    };

    let op = v.op;
    let outcome: Result<Result<(), Trap>, String> = catch_policy_value(AssertUnwindSafe(|| {
        for inst in &insts {
            emu.exec_one(inst)?;
        }
        Ok(())
    }));
    match outcome {
        Err(field) => (
            Prediction::Unknown(field.clone()),
            Prediction::Unknown(field),
        ),
        Ok(Err(trap)) => (Prediction::Trap(trap.cause), Prediction::Bits(emu.cpu.fsr)),
        Ok(Ok(())) => {
            let bits = if op.writes_boolean() {
                Prediction::Bits(u32::from(emu.cpu.b(DEST_B)))
            } else if op.writes_integer() {
                Prediction::Bits(emu.cpu.a(DEST_INT_A))
            } else {
                Prediction::Bits(emu.cpu.f(DEST_F))
            };
            (bits, Prediction::Bits(emu.cpu.fsr))
        }
    }
}

/// Run `f`, converting an unresolved-policy panic into the field's name. Any
/// other panic is re-raised, so a real bug stays a real failure.
fn catch_policy_value<T>(f: AssertUnwindSafe<impl FnOnce() -> T>) -> Result<T, String> {
    lp_xt_emu::fp_policy::suppress_unresolved_panic_output();
    let r = std::panic::catch_unwind(f);
    match r {
        Ok(v) => Ok(v),
        Err(e) => {
            let msg = e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_string()))
                .unwrap_or_else(|| "<non-string panic payload>".to_string());
            match parse_unresolved(&msg) {
                Some(field) => Err(field.to_string()),
                None => panic!("unexpected panic while predicting: {msg}"),
            }
        }
    }
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fp")
}

fn header(family: Family, unknown: &[(String, usize)]) -> String {
    let total_unknown: usize = unknown.iter().map(|(_, n)| n).sum();
    let mut s = String::new();
    let _ = writeln!(
        s,
        "# M6 Xtensa FP conformance corpus — {} {}",
        family.label(),
        family.name()
    );
    let _ = writeln!(
        s,
        "# generator:  lp-xt-fp-vectors, fingerprint {:#010x}, {} vectors",
        fingerprint(),
        count(family)
    );
    let _ = writeln!(
        s,
        "# predicted:  lp-xt-emu (host), BEFORE any hardware run — M6 P4, D2."
    );
    let _ = writeln!(
        s,
        "#             Never regenerate a row from device output: that inverts"
    );
    let _ = writeln!(
        s,
        "#             the test into a tautology that passes forever."
    );
    let _ = writeln!(
        s,
        "# unknown:    {total_unknown} of {}, by the policy field that closes them:",
        count(family)
    );
    for (field, n) in unknown {
        let _ = writeln!(s, "#             {n:>5}  {field}");
    }
    let _ = writeln!(s, "#");
    let _ = writeln!(s, "# --- silicon provenance (M6 P6 campaign) ---");
    let _ = writeln!(s, "# board:      XIAO-class ESP32-S3 devkit");
    let _ = writeln!(s, "# chip-rev:   esp32s3 v0.2");
    let _ = writeln!(s, "# flash:      16 MB (flashed with --flash-size 8mb)");
    let _ = writeln!(s, "# mac:        d8:3b:da:47:29:70");
    let _ = writeln!(
        s,
        "# port:       /dev/cu.usbmodem1201 at capture time (renumbers; identify by MAC)"
    );
    let _ = writeln!(
        s,
        "# firmware:   4e7a3da28728, feature test_xt_fp_conformance"
    );
    let _ = writeln!(
        s,
        "# toolchain:  espup esp toolchain esp-14.2.0_20240906 (xtensa-esp32s3-elf-gcc 14.2.0)"
    );
    let _ = writeln!(s, "# capture:    tests/fixtures/fp/captures/families.txt");
    let _ = writeln!(s, "# date:       2026-07-31");
    let _ = writeln!(s, "#");
    let _ = writeln!(
        s,
        "# Promoting these goldens from silicon was correct HERE — the emulator's"
    );
    let _ = writeln!(
        s,
        "# predictions were committed first and the capture was diffed against them,"
    );
    let _ = writeln!(
        s,
        "# which is what made the comparison meaningful. It is NOT a licence to"
    );
    let _ = writeln!(
        s,
        "# refresh a golden from device output anywhere downstream: that inverts a"
    );
    let _ = writeln!(s, "# test into a tautology that passes forever.");
    let _ = writeln!(s, "#");
    let _ = writeln!(s, "# columns: index op a b c imm fcr -> result fsr");
    let _ = writeln!(
        s,
        "# result:  a bit pattern, TRAP:<exccause>, or UNKNOWN:<policy field>"
    );
    let _ = writeln!(
        s,
        "# fsr:     the predicted FSR after the vector, cleared before it — a"
    );
    let _ = writeln!(
        s,
        "#          first-class prediction since P6 measured the flag semantics."
    );
    s
}

fn row(v: &Vector, p: &Prediction, fsr: &Prediction) -> String {
    format!(
        "{:05} {:<10} {:#010x} {:#010x} {:#010x} {:>2} {} -> {} {}",
        v.index,
        v.op.name(),
        v.a,
        v.b,
        v.c,
        v.imm,
        v.fcr,
        p.render(),
        fsr.render()
    )
}

/// The committed predictions for one family.
///
/// Parsed by [`parse_predictions`] — the *same* parser the campaign's diff tool
/// uses (`just fp-diff`), so a corpus file this replay accepts and the diff tool
/// chokes on cannot exist.
type FixtureRow = (u32, Prediction, Prediction);

fn read_fixture(family: Family) -> Option<(u32, Vec<FixtureRow>)> {
    let text =
        std::fs::read_to_string(fixtures_dir().join(format!("{}.txt", family.name()))).ok()?;
    let p = parse_predictions(family.name(), &text).expect("committed corpus must parse");
    Some((
        p.fingerprint,
        p.rows
            .into_iter()
            .map(|(i, _, pred, fsr)| (i, pred, fsr))
            .collect(),
    ))
}

fn write_fixture(family: Family, rows: &[(Vector, Prediction, Prediction)]) {
    // Grouped by field rather than counted in bulk, so any future unknown
    // says exactly which policy question reopened.
    let mut by_field: Vec<(String, usize)> = Vec::new();
    for (_, p, _) in rows {
        if let Prediction::Unknown(f) = p {
            match by_field.iter_mut().find(|(n, _)| n == f) {
                Some((_, n)) => *n += 1,
                None => by_field.push((f.clone(), 1)),
            }
        }
    }
    by_field.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut out = header(family, &by_field);
    for (v, p, fsr) in rows {
        out.push_str(&row(v, p, fsr));
        out.push('\n');
    }
    let dir = fixtures_dir();
    std::fs::create_dir_all(&dir).expect("fixtures dir");
    std::fs::write(dir.join(format!("{}.txt", family.name())), out).expect("write fixture");
}

fn predict_family(family: Family) -> Vec<(Vector, Prediction, Prediction)> {
    let mut emu = Emulator::new();
    (0..count(family))
        .map(|i| {
            let v = vector(family, i);
            let (p, fsr) = predict(&mut emu, &v);
            (v, p, fsr)
        })
        .collect()
}

/// The replay. Green with no board attached, forever.
#[test]
fn emulator_matches_the_committed_predictions() {
    let update = std::env::var_os("UPDATE_FP_GOLDENS").is_some();
    let mut total_unknown = 0usize;
    let mut total_rows = 0usize;
    let mut missing = Vec::new();

    for family in Family::ALL {
        let rows = predict_family(family);
        if update {
            write_fixture(family, &rows);
        }
        let Some((fp, committed)) = read_fixture(family) else {
            missing.push(family);
            continue;
        };

        // A generator change that silently invalidates the corpus must fail
        // here, not at the next hardware session.
        assert_eq!(
            fp,
            fingerprint(),
            "{}: the committed corpus was generated by a different \
             lp-xt-fp-vectors. Regenerate with UPDATE_FP_GOLDENS=1 and review \
             the diff — every prediction in it is now unverified.",
            family.name()
        );
        assert_eq!(
            committed.len(),
            rows.len(),
            "{}: row count changed",
            family.name()
        );

        for ((v, got, got_fsr), (idx, want, want_fsr)) in rows.iter().zip(&committed) {
            assert_eq!(v.index, *idx, "{}: row order drifted", family.name());
            assert_eq!(
                (got, got_fsr),
                (want, want_fsr),
                "{} vector {} ({} a={:#010x} b={:#010x} c={:#010x} imm={} fcr={})",
                family.name(),
                v.index,
                v.op.name(),
                v.a,
                v.b,
                v.c,
                v.imm,
                v.fcr
            );
            if matches!(got, Prediction::Unknown(_)) {
                total_unknown += 1;
            }
            total_rows += 1;
        }
    }

    assert!(
        missing.is_empty(),
        "no committed predictions for {missing:?} — run with UPDATE_FP_GOLDENS=1"
    );

    println!("fp_conformance: {total_rows} rows, {total_unknown} UNKNOWN");
    // The guard flipped direction at P6: before the campaign, zero unknowns
    // would have meant the policy layer quietly acquired defaults. After it,
    // every policy field is measured — so an UNKNOWN reappearing means a
    // field lost its measurement, which is exactly as bad.
    assert_eq!(
        total_unknown, 0,
        "the campaign measured every policy field; an UNKNOWN row means one \
         was un-resolved without re-running the campaign"
    );
}

/// Every `UNKNOWN` must name a field that actually exists on the policy, so a
/// typo cannot create a phantom question no campaign will ever close. (Post-P6
/// the corpus has no unknowns; this guards any future field.)
#[test]
fn every_unknown_names_a_real_policy_field() {
    let names: Vec<&'static str> = Emulator::new()
        .fp_policy
        .inventory()
        .into_iter()
        .map(|(n, ..)| n)
        .collect();
    for family in Family::ALL {
        for (v, p, _) in predict_family(family) {
            if let Prediction::Unknown(field) = p {
                assert!(
                    names.contains(&field.as_str()),
                    "{} vector {} names a policy field that does not exist: {field}",
                    family.name(),
                    v.index
                );
            }
        }
    }
}
