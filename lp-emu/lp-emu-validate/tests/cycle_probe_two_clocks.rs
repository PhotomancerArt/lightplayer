//! The `cycle-probe` payload's two clocks, asserted over the committed
//! silicon transcript.
//!
//! Every kernel is bracketed by the PMU cycle counter (`mpccr`) and by
//! SYSTIMER microseconds, read independently — the microsecond reads outside
//! the cycle reads on both sides. The two are not a check on each other in
//! the ordinary sense: they exist so that "the model disagrees with silicon"
//! can be told apart from "the counter did something odd". Plan one lost a
//! sitting to that ambiguity, and the answer turned out to be a link
//! mismatch rather than a counter that paused (`notes.md` F3).
//!
//! This is the gate that keeps the claim honest. At 160 MHz,
//! `cycles / 160e6` and `us / 1e6` must be the same span. **If they ever stop
//! agreeing, that is a finding to report, not a threshold to widen.**
//!
//! ```bash
//! cargo run -q -p lp-cli -- validate record cycle-probe \
//!   --config silicon:esp32c6 --port /dev/cu.usbmodem1433201 \
//!   --commit b89893962c76 --date 2026-09-08
//! ```
//!
//! **Never edit a transcript.** A failure here is a regression or a
//! re-capture with its own header, never a digit changed in a `.txt`.

use std::path::PathBuf;

use lp_emu_validate::transcript::Transcript;

/// The desk board, `A0:F2:62:87:B4:8C`, 2026-09-08.
const SILICON: &str = "silicon-esp32c6-2026-09-08-b89893962.txt";
/// The emulator twins at the same commit, on the same image.
const T1: &str = "lp-emu-esp32c6-t1-2026-09-08-b89893962.txt";
const T2: &str = "lp-emu-esp32c6-t2-2026-09-08-b89893962.txt";

/// `board/esp32c6/constants::CPU_HZ`, as cycles per microsecond.
const CYCLES_PER_US: f64 = 160.0;

/// Spans long enough that the bracket's own cost cannot matter: 1 ms is a
/// thousand times the measured bracket overhead.
const LONG_ENOUGH_CYCLES: u64 = 160_000;

/// What the bracket itself adds to `us` and not to `cycles`: the two
/// `Instant::now()` calls sit outside the two `mpccr` reads. Measured on the
/// committed capture as `bracket_overhead`, whose own `us` runs 1–2 µs
/// against a cycle span of 0.3 µs. `as_micros` truncates, so a whole
/// microsecond of quantisation rides on top; three is the ceiling that covers
/// both and it is asserted below rather than assumed.
const BRACKET_US_CEILING: f64 = 3.0;

fn load(name: &str) -> Transcript {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../transcripts/esp32c6/cycle-probe")
        .join(name);
    Transcript::load(&path).unwrap_or_else(|e| panic!("loading {}: {e:#}", path.display()))
}

struct Reading {
    kernel: String,
    rep: u64,
    cycles: u64,
    us: u64,
}

fn readings(t: &Transcript) -> Vec<Reading> {
    t.records_of("cycle-probe")
        .expect("the cycle-probe records parse")
        .into_iter()
        .map(|r| Reading {
            kernel: r
                .get("kernel")
                .and_then(|v| v.as_str())
                .expect("kernel")
                .to_string(),
            rep: r.get("rep").and_then(|v| v.as_u64()).expect("rep"),
            cycles: r.get("cycles").and_then(|v| v.as_u64()).expect("cycles"),
            us: r.get("us").and_then(|v| v.as_u64()).expect("us"),
        })
        .collect()
}

/// **The gate.** For every kernel whose span is a millisecond or more, the
/// cycle counter and SYSTIMER agree to within 1 %.
#[test]
fn the_two_clocks_agree_on_silicon() {
    let t = load(SILICON);
    let mut checked = 0usize;
    let mut worst = 0.0f64;
    let mut worst_name = String::new();
    for r in readings(&t) {
        if r.cycles < LONG_ENOUGH_CYCLES {
            continue;
        }
        let implied_us = r.cycles as f64 / CYCLES_PER_US;
        let delta = (implied_us - r.us as f64).abs() / r.us as f64 * 100.0;
        assert!(
            delta <= 1.0,
            "{} rep {}: mpccr says {implied_us:.1} us, SYSTIMER says {} us ({delta:.3} % apart). \
             This is the phase's finding if it fires — report it, do not widen the bound.",
            r.kernel,
            r.rep,
            r.us,
        );
        if delta > worst {
            worst = delta;
            worst_name = format!("{} rep {}", r.kernel, r.rep);
        }
        checked += 1;
    }
    assert!(checked >= 60, "only {checked} readings were long enough");
    assert!(
        worst < 1.0,
        "worst agreement was {worst:.4} % on {worst_name}"
    );
}

/// The kernels *below* a millisecond are not exempt — they are explained.
///
/// `bracket_overhead` and `slice_shape` are short enough that the bracket's
/// own cost is a visible fraction of them, so a percentage bound would fail
/// for a reason that has nothing to do with either clock. The claim that
/// holds for them is absolute rather than relative: `us` exceeds the cycle
/// span by no more than the bracket costs, which is what
/// [`BRACKET_US_CEILING`] is. If a short kernel ever comes back *shorter* on
/// SYSTIMER than on `mpccr`, that is a counter anomaly and this fails.
///
/// The one reading held to a different bound is the run's **first**, and
/// [`the_first_bracket_is_the_cold_one`] is why.
#[test]
fn the_short_kernels_differ_by_the_bracket_and_nothing_more() {
    let t = load(SILICON);
    let mut short = 0usize;
    for r in readings(&t) {
        if r.cycles >= LONG_ENOUGH_CYCLES || (r.kernel == "bracket_overhead" && r.rep == 0) {
            continue;
        }
        let implied_us = r.cycles as f64 / CYCLES_PER_US;
        let excess = r.us as f64 - implied_us;
        assert!(
            (-1.0..=BRACKET_US_CEILING).contains(&excess),
            "{} rep {}: SYSTIMER {} us against mpccr's {implied_us:.2} us — {excess:.2} us apart, \
             which the bracket cannot account for",
            r.kernel,
            r.rep,
            r.us,
        );
        short += 1;
    }
    assert!(short > 0, "no short kernels in the capture");
}

/// `bracket_overhead` repetition 0 is the first bracket of the whole run, and
/// it is cold in a way nothing after it is.
///
/// On the committed silicon capture it reads **884 cycles against 42** for
/// the repetitions after it, and its `us` runs about 5 µs beyond its cycle
/// span. Both are the same fact: on the first pass the measurement code
/// itself is being fetched from flash — the inner `mpccr` reads and, outside
/// them, the two `Instant::now()` calls. `code_walk` puts a number on what
/// that fetch costs, and it is not small.
///
/// So this reading is held to a looser absolute bound than its siblings, and
/// this test is the evidence for the exemption rather than a note asserting
/// it: if the first bracket ever stops being conspicuously cold, the
/// exemption stops being earned and this fails.
#[test]
fn the_first_bracket_is_the_cold_one() {
    let t = load(SILICON);
    let brackets: Vec<Reading> = readings(&t)
        .into_iter()
        .filter(|r| r.kernel == "bracket_overhead")
        .collect();
    assert_eq!(brackets.len(), 5);

    let first = &brackets[0];
    let rest_max = brackets[1..].iter().map(|r| r.cycles).max().unwrap();
    assert!(
        first.cycles >= rest_max * 10,
        "the first bracket cost {} cycles against a later {rest_max} — it is no longer the \
         cold one, so its looser microsecond bound is no longer earned",
        first.cycles,
    );

    let implied_us = first.cycles as f64 / CYCLES_PER_US;
    let excess = first.us as f64 - implied_us;
    assert!(
        (0.0..=12.0).contains(&excess),
        "the first bracket's SYSTIMER reading is {excess:.2} us beyond its cycle span, which a \
         cold fetch of the bracket's own code does not explain",
    );
}

/// And on the emulator too, where the two clocks are modelled from one
/// counter — so this is a check on the SYSTIMER model's divisor rather than
/// on a chip.
#[test]
fn the_two_clocks_agree_on_both_emulated_grades() {
    for name in [T1, T2] {
        let t = load(name);
        for r in readings(&t) {
            if r.cycles < LONG_ENOUGH_CYCLES {
                continue;
            }
            let implied_us = r.cycles as f64 / CYCLES_PER_US;
            let delta = (implied_us - r.us as f64).abs() / r.us as f64 * 100.0;
            assert!(
                delta <= 1.0,
                "{name}: {} rep {}: {implied_us:.1} us against {} us ({delta:.3} %)",
                r.kernel,
                r.rep,
                r.us,
            );
        }
    }
}

/// Both machines ran the same instructions, and `acc` is how we know.
///
/// Every kernel's accumulator is arithmetic over constants read back out of a
/// `black_box`. If silicon and an emulated grade disagree on one, the cycle
/// columns beside it are not two measurements of one kernel and the whole
/// calibration is void — which is why `acc` is graded `Structural` and not
/// `Timing`.
#[test]
fn every_kernel_computed_the_same_thing_on_every_machine() {
    let silicon = load(SILICON);
    let want: Vec<(String, u64, i64)> = silicon
        .records_of("cycle-probe")
        .expect("records")
        .into_iter()
        .map(|r| {
            (
                r.get("kernel").unwrap().as_str().unwrap().to_string(),
                r.get("rep").unwrap().as_u64().unwrap(),
                r.get("acc").unwrap().as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(want.len(), 80, "16 kernels x 5 repetitions");

    for name in [T1, T2] {
        let got: Vec<(String, u64, i64)> = load(name)
            .records_of("cycle-probe")
            .expect("records")
            .into_iter()
            .map(|r| {
                (
                    r.get("kernel").unwrap().as_str().unwrap().to_string(),
                    r.get("rep").unwrap().as_u64().unwrap(),
                    r.get("acc").unwrap().as_i64().unwrap(),
                )
            })
            .collect();
        assert_eq!(got, want, "{name} did not run silicon's kernels");
    }
}

/// The declared instruction counts are the assembly's, and `t1` is the
/// witness: it charges exactly one cycle per instruction, so on `t1` a
/// kernel's cycles are its instruction count plus the bracket's handful.
///
/// A wrong `insns_per_iter` in the harness would put a fabricated
/// cycles-per-instruction in the calibration report and nothing else would
/// notice. This is what notices.
#[test]
fn the_declared_instruction_counts_are_what_t1_counted() {
    let t = load(T1);
    let mut checked = 0usize;
    for r in t.records_of("cycle-probe").expect("records") {
        let Some(insns) = r.get("insns").and_then(|v| v.as_u64()) else {
            continue;
        };
        if insns == 0 {
            continue;
        }
        let cycles = r.get("cycles").and_then(|v| v.as_u64()).expect("cycles");
        let kernel = r.get("kernel").and_then(|v| v.as_str()).expect("kernel");
        let overhead = cycles as i64 - insns as i64;
        assert!(
            (0..=200).contains(&overhead),
            "{kernel}: t1 charged {cycles} cycles for a declared {insns} instructions — \
             the declared count is wrong, or t1 stopped charging one cycle each"
        );
        checked += 1;
    }
    assert_eq!(checked, 40, "eight assembly kernels x five repetitions");
}
