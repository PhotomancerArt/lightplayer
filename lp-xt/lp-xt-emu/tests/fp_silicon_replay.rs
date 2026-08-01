//! The silicon replay: the M6 P6 captures, re-verified against the emulator on
//! every test run, with **no board attached, forever**.
//!
//! Three committed captures from the desk ESP32-S3 (chip rev v0.2, MAC
//! `d8:3b:da:47:29:70`, 2026-07-31) live under `tests/fixtures/fp/captures/`:
//!
//! - `tables.txt` — 60 run-length-encoded estimate-ROM sweeps (the full 2²³
//!   significand space over 15 `(sign, exponent)` planes per op).
//! - `helpers.txt` — 5 328 divide-step helper probes plus the sixteen
//!   `const.s` outputs.
//! - `families.txt` — all 5 630 conformance vectors, results and FSR.
//!
//! Each test replays a capture through the emulator (or the `fp_rom` model)
//! and asserts **exact** agreement. This is what "the emulator is trusted"
//! means mechanically: the campaign's measurements stay load-bearing, and a
//! regression that would have diverged from silicon fails here instead of at
//! the next desk session.
//!
//! **Never edit a capture.** They are verbatim silicon transcripts (filtered
//! to their `[FPCONF]` lines); a mismatch here is an emulator regression, not
//! a fixture to refresh.

use std::path::PathBuf;

use lp_xt_emu::cpu::CPENABLE_FPU;
use lp_xt_emu::fp_capture::{Capture, DeviceResult, parse_capture};
use lp_xt_emu::{Emulator, fp_rom};
use lp_xt_fp_vectors::helpers::{self, HelperOp, probe2};
use lp_xt_inst::{FReg, FpRrOp, FpRrrOp, Inst};

fn capture(name: &str) -> Capture {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fp/captures")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    parse_capture(&text).unwrap_or_else(|e| panic!("{path:?}: {e}"))
}

/// The estimate model reproduces every RLE run of every sweep: both endpoints
/// of every run, plus strided interior points. (The exhaustive all-2²³ check
/// is `estimate_roms_reproduce_every_sweep_exhaustively` below, `#[ignore]`d
/// for cost; its full run is recorded in the campaign record.)
#[test]
fn estimate_roms_reproduce_every_sweep_at_run_boundaries() {
    let cap = capture("tables.txt");
    assert_eq!(cap.tables.len(), 60, "60 sweeps were captured");
    let mut checked = 0u64;
    for sweep in &cap.tables {
        assert!(!sweep.aborted, "no captured sweep aborted");
        let f = model_for(&sweep.op);
        let mut covered = 0u64;
        for &(start, len, want) in &sweep.runs {
            let mut points = vec![start, start + len - 1, start + len / 2];
            let mut p = 0x1000;
            while p < len {
                points.push(start + p);
                p += 0x1000;
            }
            for frac in points {
                let input = (sweep.sign << 31) | (sweep.exp << 23) | frac;
                let (got, _) = f(input);
                assert_eq!(
                    got, want,
                    "{} sign={} exp={} frac={frac:#08x}",
                    sweep.op, sweep.sign, sweep.exp
                );
                checked += 1;
            }
            covered += u64::from(len);
        }
        assert_eq!(covered, 1 << 23, "{}: full plane coverage", sweep.op);
    }
    println!("estimate replay: {checked} points across 60 sweeps");
}

/// The full 2²³-per-plane check — ~503M model evaluations. Run manually:
/// `cargo test -p lp-xt-emu --release --test fp_silicon_replay -- --ignored`
#[test]
#[ignore = "exhaustive (~503M evaluations); the boundary test runs in CI"]
fn estimate_roms_reproduce_every_sweep_exhaustively() {
    let cap = capture("tables.txt");
    for sweep in &cap.tables {
        let f = model_for(&sweep.op);
        for &(start, len, want) in &sweep.runs {
            for frac in start..start + len {
                let input = (sweep.sign << 31) | (sweep.exp << 23) | frac;
                assert_eq!(
                    f(input).0,
                    want,
                    "{} sign={} exp={} frac={frac:#08x}",
                    sweep.op,
                    sweep.sign,
                    sweep.exp
                );
            }
        }
    }
}

fn model_for(op: &str) -> fn(u32) -> (u32, u32) {
    match op {
        "recip0.s" => fp_rom::recip0,
        "rsqrt0.s" => fp_rom::rsqrt0,
        "sqrt0.s" => fp_rom::sqrt0,
        "div0.s" => fp_rom::div0,
        other => panic!("unknown estimate op {other}"),
    }
}

/// Run one helper probe through the real executor path.
fn run_probe(emu: &mut Emulator, kernel: u8, r: u32, s: u32, t: u32) -> (u32, u32) {
    emu.cpu.fr = [0; 16];
    emu.cpu.fsr = 0;
    emu.cpu.fcr = 0;
    emu.cpu.cpenable = CPENABLE_FPU;
    emu.cpu.set_f(0, r);
    emu.cpu.set_f(1, s);
    emu.cpu.set_f(2, t);
    let f = FReg::new;
    let inst = match kernel {
        0 => Inst::FpRr(FpRrOp::Nexp01S, f(0), f(1)),
        1 => Inst::FpRr(FpRrOp::MksadjS, f(0), f(1)),
        2 => Inst::FpRr(FpRrOp::MkdadjS, f(0), f(1)),
        3 => Inst::FpRr(FpRrOp::AddexpS, f(0), f(1)),
        4 => Inst::FpRr(FpRrOp::AddexpmS, f(0), f(1)),
        5 => Inst::FpRrr(FpRrrOp::MaddnS, f(0), f(1), f(2)),
        6 => Inst::FpRrr(FpRrrOp::DivnS, f(0), f(1), f(2)),
        _ => Inst::FpRrr(FpRrrOp::MaddS, f(0), f(1), f(2)),
    };
    emu.exec_one(&inst).expect("no trap");
    (emu.cpu.f(0), emu.cpu.fsr)
}

fn helper_kernel_id(op: HelperOp) -> u8 {
    match op {
        HelperOp::Nexp01S => 0,
        HelperOp::MksadjS => 1,
        HelperOp::MkdadjS => 2,
        HelperOp::AddexpS => 3,
        HelperOp::AddexpmS => 4,
        HelperOp::MaddnS => 5,
        HelperOp::DivnS => 6,
        HelperOp::MaddS => 7,
    }
}

/// Every first-round helper probe, replayed exactly — results AND flags — for
/// the seven fully-modeled ops. `divn.s` is the honest exception: its model
/// is exact on the divide/sqrt-sequence envelope (families replay below
/// proves all 272 end-to-end rows) but not across the whole probe plane; its
/// agreement count is **pinned** so any drift in either direction is loud.
#[test]
fn helper_probes_replay_exactly() {
    let cap = capture("helpers.txt");
    assert_eq!(cap.helpers_fingerprint, Some(helpers::fingerprint()));

    // const.s: the measured sixteen.
    let want_const: Vec<(u8, u32)> = (0u8..16)
        .map(|i| {
            (
                i,
                [0x0000_0000u32, 0x3F80_0000, 0x4000_0000, 0x3F00_0000][usize::from(i) & 3],
            )
        })
        .collect();
    assert_eq!(cap.const_s, want_const, "const.s table");

    let mut emu = Emulator::new();
    let mut divn_match = 0u32;
    let mut divn_total = 0u32;
    for op in HelperOp::ALL {
        let rows = &cap.rows[op.name()];
        assert_eq!(rows.len() as u32, helpers::count(op), "{}", op.name());
        for (&i, res) in rows {
            let DeviceResult::Value { bits, fsr } = *res else {
                panic!("helper rows are never skipped")
            };
            let v = helpers::probe(op, i);
            let (got, got_fsr) = run_probe(&mut emu, helper_kernel_id(op), v.r, v.s, v.t);
            if op == HelperOp::DivnS {
                divn_total += 1;
                if (got, got_fsr) == (bits, fsr) {
                    divn_match += 1;
                }
                continue;
            }
            assert_eq!(
                (got, got_fsr),
                (bits, fsr),
                "{} probe {i}: r={:#010x} s={:#010x} t={:#010x}",
                op.name(),
                v.r,
                v.s,
                v.t
            );
        }
    }
    println!("helper replay: divn {divn_match}/{divn_total} probe rows match");
    // Pinned, not asserted >= : an "improvement" without a re-fit against the
    // capture would be as suspicious as a regression.
    assert_eq!(
        (divn_match, divn_total),
        (DIVN_PROBE_MATCHES, 1536),
        "divn.s probe agreement moved — re-fit against the capture and \
         update the campaign record before repinning"
    );
}

/// The exact number of first-round divn probe rows the model reproduces. The
/// remainder are off the sequence envelope (operand shapes no sequence can
/// produce); the campaign record's divn section discusses them, and the
/// second-round probe grids (queued behind a board replug) exist to close
/// them.
const DIVN_PROBE_MATCHES: u32 = 1387;

/// The families capture replayed against the committed predictions via the
/// campaign's own diff tool: 5 630 rows, zero divergence, results and FSR.
#[test]
fn the_families_capture_agrees_with_the_predictions_completely() {
    use lp_xt_emu::fp_capture::{diff, parse_predictions};
    use lp_xt_fp_vectors::Family;
    let cap = capture("families.txt");
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fp");
    let (mut compared, mut agree) = (0, 0);
    for family in Family::ALL {
        let text = std::fs::read_to_string(fixtures.join(format!("{}.txt", family.name())))
            .expect("committed corpus");
        let pred = parse_predictions(family.name(), &text).expect("parses");
        let report = diff(&pred, &cap).expect("diffable");
        assert_eq!(
            (report.diverge, report.resolved, report.skipped),
            (0, 0, 0),
            "{}: {report}",
            family.name()
        );
        compared += report.compared;
        agree += report.agree;
    }
    assert_eq!((compared, agree), (5630, 5630));
}

/// The second-round probe grids exist and are fingerprint-stable, so the
/// queued board session (blocked on a physical replug at G2) captures against
/// exactly this generator.
#[test]
fn probe2_grids_are_ready_for_the_next_board_window() {
    assert_eq!(probe2::fingerprint(), 0x67C2_9B75);
    assert_eq!(probe2::total(), 7073);
}
