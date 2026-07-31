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

/// What the emulator predicts for one vector.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Prediction {
    /// A concrete answer: IEEE-fixed, or resolved policy.
    Bits(u32),
    /// The instruction faulted; the value is the EXCCAUSE.
    Trap(u32),
    /// The prediction needs a policy field nothing has measured yet.
    Unknown(String),
}

impl Prediction {
    fn render(&self) -> String {
        match self {
            Prediction::Bits(b) => format!("{b:#010x}"),
            Prediction::Trap(c) => format!("TRAP:{c}"),
            Prediction::Unknown(f) => format!("UNKNOWN:{f}"),
        }
    }

    fn parse(s: &str) -> Prediction {
        if let Some(f) = s.strip_prefix("UNKNOWN:") {
            Prediction::Unknown(f.to_string())
        } else if let Some(c) = s.strip_prefix("TRAP:") {
            Prediction::Trap(c.parse().expect("trap cause"))
        } else {
            Prediction::Bits(u32::from_str_radix(s.trim_start_matches("0x"), 16).expect("bits"))
        }
    }
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

/// Run one vector on a fresh-state emulator and report what it predicts.
fn predict(emu: &mut Emulator, v: &Vector) -> Prediction {
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

    let Some(inst) = instruction(v) else {
        // The manual's divide and square-root sequences. Predicting them needs
        // the semantics of the non-estimate helper steps, so the harness reads
        // that field and lets it name itself. This is a derivation, not a
        // hand-written classification: when P6 resolves the field, these rows
        // stop being UNKNOWN without anyone editing this file.
        return match catch_policy(|| {
            emu.fp_policy.divide_step_helpers.get();
        }) {
            Ok(()) => unreachable!("divide_step_helpers resolved but no sequence executor exists"),
            Err(field) => Prediction::Unknown(field),
        };
    };

    let op = v.op;
    let outcome: Result<Result<(), Trap>, String> =
        catch_policy_value(AssertUnwindSafe(|| emu.exec_one(&inst)));
    match outcome {
        Err(field) => Prediction::Unknown(field),
        Ok(Err(trap)) => Prediction::Trap(trap.cause),
        Ok(Ok(())) => {
            if op.writes_boolean() {
                Prediction::Bits(u32::from(emu.cpu.b(DEST_B)))
            } else if op.writes_integer() {
                Prediction::Bits(emu.cpu.a(DEST_INT_A))
            } else {
                Prediction::Bits(emu.cpu.f(DEST_F))
            }
        }
    }
}

/// Run `f`, converting an unresolved-policy panic into the field's name. Any
/// other panic is re-raised, so a real bug stays a real failure.
fn catch_policy(f: impl FnOnce()) -> Result<(), String> {
    catch_policy_value(AssertUnwindSafe(f))
}

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

fn header(family: Family, unknown: usize) -> String {
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
    let _ = writeln!(s, "# unknown:    {unknown} of {}", count(family));
    let _ = writeln!(s, "#");
    let _ = writeln!(s, "# --- silicon provenance (M6 P6 fills this in) ---");
    let _ = writeln!(s, "# board:      NOT RUN");
    let _ = writeln!(s, "# chip-rev:   NOT RUN");
    let _ = writeln!(s, "# flash:      NOT RUN");
    let _ = writeln!(s, "# mac:        NOT RUN");
    let _ = writeln!(s, "# port:       NOT RUN");
    let _ = writeln!(s, "# firmware:   NOT RUN");
    let _ = writeln!(s, "# toolchain:  NOT RUN");
    let _ = writeln!(s, "# date:       NOT RUN");
    let _ = writeln!(s, "#");
    let _ = writeln!(s, "# columns: index op a b c imm fcr -> result fsr");
    let _ = writeln!(
        s,
        "# result:  a bit pattern, TRAP:<exccause>, or UNKNOWN:<policy field>"
    );
    let _ = writeln!(
        s,
        "# fsr:     UNKNOWN everywhere — FSR accumulates (measured, M6 P1) but"
    );
    let _ = writeln!(
        s,
        "#          neither the flag layout nor which op sets which flag is known."
    );
    s
}

fn row(v: &Vector, p: &Prediction) -> String {
    format!(
        "{:05} {:<10} {:#010x} {:#010x} {:#010x} {:>2} {} -> {} UNKNOWN:fsr_flag_bits",
        v.index,
        v.op.name(),
        v.a,
        v.b,
        v.c,
        v.imm,
        v.fcr,
        p.render()
    )
}

/// The committed predictions for one family: `index -> (rendered row, result)`.
fn read_fixture(family: Family) -> Option<(u32, Vec<(u32, Prediction)>)> {
    let text =
        std::fs::read_to_string(fixtures_dir().join(format!("{}.txt", family.name()))).ok()?;
    let mut fp = 0u32;
    let mut rows = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# generator:") {
            let tok = rest
                .split_whitespace()
                .find(|t| t.starts_with("0x"))
                .expect("fingerprint in the header");
            fp = u32::from_str_radix(tok.trim_end_matches(',').trim_start_matches("0x"), 16)
                .expect("fingerprint hex");
        }
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let (lhs, rhs) = line.split_once("->").expect("every row has an arrow");
        let index: u32 = lhs.split_whitespace().next().unwrap().parse().unwrap();
        let result = rhs.split_whitespace().next().unwrap();
        rows.push((index, Prediction::parse(result)));
    }
    Some((fp, rows))
}

fn write_fixture(family: Family, rows: &[(Vector, Prediction)]) {
    let unknown = rows
        .iter()
        .filter(|(_, p)| matches!(p, Prediction::Unknown(_)))
        .count();
    let mut out = header(family, unknown);
    for (v, p) in rows {
        out.push_str(&row(v, p));
        out.push('\n');
    }
    let dir = fixtures_dir();
    std::fs::create_dir_all(&dir).expect("fixtures dir");
    std::fs::write(dir.join(format!("{}.txt", family.name())), out).expect("write fixture");
}

fn predict_family(family: Family) -> Vec<(Vector, Prediction)> {
    let mut emu = Emulator::new();
    (0..count(family))
        .map(|i| {
            let v = vector(family, i);
            let p = predict(&mut emu, &v);
            (v, p)
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

        for ((v, got), (idx, want)) in rows.iter().zip(&committed) {
            assert_eq!(v.index, *idx, "{}: row order drifted", family.name());
            assert_eq!(
                got,
                want,
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

    println!(
        "fp_conformance: {total_rows} rows, {total_unknown} UNKNOWN \
         ({:.1}%) — each one a question for the M6 P6 silicon campaign",
        100.0 * total_unknown as f64 / total_rows as f64
    );
    // Zero unknowns before P6 would mean the policy layer had quietly acquired
    // defaults, which is the failure this milestone exists to prevent.
    assert!(
        total_unknown > 0,
        "the unknown count is zero before the hardware campaign — the policy \
         layer must have acquired defaults"
    );
}

/// The IEEE-fixed part of the corpus must actually be predicted, or the whole
/// thing would be one big `UNKNOWN` and the campaign would measure nothing the
/// emulator could be wrong about.
#[test]
fn a_substantial_share_of_the_corpus_is_concretely_predicted() {
    let mut concrete = 0usize;
    let mut total = 0usize;
    for family in Family::ALL {
        for (_, p) in predict_family(family) {
            if matches!(p, Prediction::Bits(_)) {
                concrete += 1;
            }
            total += 1;
        }
    }
    let share = concrete as f64 / total as f64;
    println!(
        "fp_conformance: {concrete}/{total} concretely predicted ({:.1}%)",
        share * 100.0
    );
    assert!(
        share > 0.20,
        "only {:.1}% of the corpus is concretely predicted — the IEEE-fixed \
         core should be a large share of it",
        share * 100.0
    );
}

/// Every `UNKNOWN` must name a field that actually exists on the policy, so a
/// typo cannot create a phantom question that P6 will never close.
#[test]
fn every_unknown_names_a_real_policy_field() {
    let names: Vec<&'static str> = Emulator::new()
        .fp_policy
        .inventory()
        .into_iter()
        .map(|(n, ..)| n)
        .collect();
    for family in Family::ALL {
        for (v, p) in predict_family(family) {
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
