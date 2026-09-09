//! The timing band, as a contract rather than a paragraph (M1 P4).
//!
//! `--strict-timing` compared timing fields for **equality** until this
//! phase, and a cycle model is never equal — so `timing` could only ever be
//! `modeled` however good the model got, and the grade stopped carrying
//! information. A trust entry may now state a **band**: an interval the
//! per-sample ratio must fall in, a fraction of samples that must fall in it,
//! an aggregate tolerance, and the payloads that measured it.
//!
//! What this file keeps true:
//!
//! * **Additive.** A configuration with no band behaves exactly as it did
//!   before the field existed, and every committed sidecar still loads.
//! * **The `on` list is the boundary.** A band does not travel to a payload
//!   nobody measured it on; there, the comparison is exact, as before.
//! * **The band is not a mask.** A timing figure moved far enough fails, a
//!   figure moved a little does not, and a **memory** figure fails either
//!   way — a band is a statement about a class that is never exact, and
//!   memory is not that class.
//! * **The image is pinned** (RD4/OQ7). These replays need no firmware build,
//!   so they cost nothing and run everywhere; what they must not do is
//!   silently start comparing against a transcript recorded from some future
//!   PR's own image. The recorded `firmware_sha256` is asserted against the
//!   constants the calibration record names.
//!
//! **Never edit a transcript.** The perturbations below rebuild a transcript
//! in memory from the committed bytes; nothing here writes to the tree.

use std::path::PathBuf;

use lp_emu_validate::TranscriptHeader;
use lp_emu_validate::configuration::{Band, TrustEntry, TrustTable};
use lp_emu_validate::grade::{FieldClass, Grade};
use lp_emu_validate::replay::{ReplayOptions, ReplayReport, replay};
use lp_emu_validate::transcript::{Transcript, sidecar_path};

const COMPILE: &str = "shader-compile-stress";
const PROBE: &str = "cycle-probe";

/// The `t3` recordings the calibration record's §3 and §4 are derived from.
const T3_COMPILE: &str = "lp-emu-esp32c6-t3-2026-09-08-17ac011f7.txt";
const T3_PROBE: &str = "lp-emu-esp32c6-t3-2026-09-08-17ac011f7.txt";
/// The desk board, running the same image bytes over the same link.
const SILICON_COMPILE: &str = "silicon-esp32c6-2026-09-07-735af98ae.txt";
const SILICON_PROBE: &str = "silicon-esp32c6-2026-09-08-b89893962.txt";
/// A grade with no band, for the "exactly as before" half.
const T1_COMPILE: &str = "lp-emu-esp32c6-t1-2026-09-08-773ebf997.txt";
const T2_COMPILE: &str = "lp-emu-esp32c6-t2-2026-09-08-773ebf997.txt";

/// RD4/OQ7: the bytes each grade-3 replay is allowed to run against.
///
/// A grade-3 gate replays two committed files and builds no firmware, which
/// is what makes it free enough to run on every PR. The risk that buys is the
/// other one: a transcript swapped for one recorded from a PR's own image
/// would move the numbers with nothing to see. These shas pin the image at
/// the transcript's commit, so a re-recording has to arrive as a new stem
/// with its own sha and its own line here — which is the same rule
/// `build-reference-image.sh` exists to make checkable
/// (`docs/debt/reference-images-are-not-reproducible-across-hosts.md`).
const SHA_T3_COMPILE: &str = "23d82aa5fbeabe230634c48a3eea804d3472f88456024eebd644eaf6dcb7fda1";
const SHA_T3_PROBE: &str = "9222e09d3515a33d0696589b5e79da8ca1efd6cd663c9a9b32fee4736920f7e7";
const SHA_SILICON_PROBE: &str = "1410bee6780988e395ea92b8c236ee3dda923a38cfe22102733af2d5d0f1b0a0";

fn transcripts() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../transcripts/esp32c6")
}

fn load(payload: &str, name: &str) -> Transcript {
    let path = transcripts().join(payload).join(name);
    Transcript::load(&path).unwrap_or_else(|e| panic!("loading {}: {e:#}", path.display()))
}

/// Reload a transcript with its body rewritten. The file on disk is never
/// touched — this is the negative control's vehicle, not an edit.
fn load_with_body(payload: &str, name: &str, edit: impl Fn(String) -> String) -> Transcript {
    let path = transcripts().join(payload).join(name);
    let body = std::fs::read_to_string(&path).unwrap();
    let header =
        TranscriptHeader::from_json(&std::fs::read_to_string(sidecar_path(&path)).unwrap())
            .unwrap();
    Transcript::from_parts(header, &edit(body)).unwrap()
}

/// Reload a transcript with its sidecar's trust table replaced. Used to reach
/// the sidecar fallback path — a configuration `validate.toml` knows but
/// states no band for.
fn load_with_trust(payload: &str, name: &str, trust: TrustTable) -> Transcript {
    let path = transcripts().join(payload).join(name);
    let body = std::fs::read_to_string(&path).unwrap();
    let mut header =
        TranscriptHeader::from_json(&std::fs::read_to_string(sidecar_path(&path)).unwrap())
            .unwrap();
    header.trust = trust;
    Transcript::from_parts(header, &body).unwrap()
}

fn strict_timing(_payload: &str, left: &Transcript, right: &Transcript) -> ReplayReport {
    replay(
        left,
        right,
        ReplayOptions {
            strict: false,
            strict_timing: true,
        },
    )
    .unwrap()
}

fn verdict<'a>(
    report: &'a ReplayReport,
    scope: &str,
    field: &str,
) -> lp_emu_validate::replay::BandVerdict {
    report
        .band_verdicts()
        .into_iter()
        .find(|v| v.scope == scope && v.field == field)
        .unwrap_or_else(|| panic!("no band verdict for {scope}.{field}"))
}

// --- G4-1: additive ------------------------------------------------------

/// Every sidecar in the tree still parses, band field or no band field.
///
/// The one that matters is the oldest: a trust entry serialised before the
/// field existed has no `band` key at all, and `#[serde(default)]` is what
/// makes that a `None` rather than a load failure.
#[test]
fn every_committed_sidecar_still_loads() {
    let mut seen = 0;
    let root = transcripts();
    for payload in std::fs::read_dir(&root).unwrap() {
        let dir = payload.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "txt") {
                continue;
            }
            Transcript::load(&path).unwrap_or_else(|e| panic!("loading {}: {e:#}", path.display()));
            seen += 1;
        }
    }
    assert!(
        seen >= 20,
        "only {seen} transcripts walked — the walk broke"
    );
}

/// A configuration with no band is compared exactly, as it always was: one
/// failure per differing timing field, and no band in the report.
#[test]
fn a_band_less_trust_entry_behaves_exactly_as_before() {
    let t1 = load(COMPILE, T1_COMPILE);
    let silicon = load(COMPILE, SILICON_COMPILE);
    let report = strict_timing(COMPILE, &t1, &silicon);

    assert!(
        report.timing_band.is_none(),
        "lp-emu:esp32c6:t1 states no band; nothing may invent one for it"
    );
    assert!(report.band_verdicts().is_empty());

    let differing = report.differences_in(FieldClass::Timing).count();
    assert_eq!(differing, 188, "the 09-08 t1 pair's timing divergences");
    let failures = report.failures();
    assert_eq!(
        failures.len(),
        differing,
        "exactly one failure per differing timing field — the pre-band rule"
    );
    assert!(
        failures[0].ends_with("(--strict-timing)"),
        "{}",
        failures[0]
    );
    assert!(!report.is_ok());

    // And the same pair without --strict-timing is still OK: the band changed
    // what fails under the flag, never what happens without it.
    let lenient = replay(&t1, &silicon, ReplayOptions::default()).unwrap();
    assert!(lenient.is_ok(), "{:?}", lenient.failures());
}

/// A band with no `on` list is refused where it is written, not where it is
/// read. A band that does not say which payloads measured it is a claim about
/// payloads nobody ran.
#[test]
fn a_band_without_an_on_list_is_refused_at_parse_time() {
    let with_on =
        r#"{"per_sample":[0.8,1.25],"per_sample_coverage":0.9,"aggregate":0.2,"on":["p"]}"#;
    serde_json::from_str::<Band>(with_on).expect("a band naming a payload parses");

    let without = r#"{"per_sample":[0.8,1.25],"per_sample_coverage":0.9,"aggregate":0.2}"#;
    let err = serde_json::from_str::<Band>(without)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("payloads nobody ran"),
        "the refusal should say why: {err}"
    );

    for (bad, want) in [
        (
            r#"{"per_sample":[1.3,1.1],"per_sample_coverage":0.9,"aggregate":0.2,"on":["p"]}"#,
            "0 < low <= high",
        ),
        (
            r#"{"per_sample":[0.8,1.25],"per_sample_coverage":1.5,"aggregate":0.2,"on":["p"]}"#,
            "fraction in (0, 1]",
        ),
        (
            r#"{"per_sample":[0.8,1.25],"per_sample_coverage":0.9,"aggregate":-0.1,"on":["p"]}"#,
            "non-negative",
        ),
    ] {
        let err = serde_json::from_str::<Band>(bad).unwrap_err().to_string();
        assert!(err.contains(want), "expected `{want}` in: {err}");
    }
}

/// `validate.toml`'s own `t3` entry, as the tree states it. The numbers are
/// Yona's to change at G1; that they are *stated* is not.
#[test]
fn the_t3_timing_entry_states_a_band_naming_both_payloads() {
    let cfg = lp_emu_validate::ValidateConfig::embedded();
    let t3 = cfg.configuration("lp-emu:esp32c6:t3").unwrap();
    let band = t3
        .trust
        .band(FieldClass::Timing)
        .expect("t3 states a timing band");
    assert!(band.covers(COMPILE) && band.covers(PROBE), "{:?}", band.on);
    assert_eq!(band.on.len(), 2, "only the payloads that measured it");

    // Every other configuration in the table is band-less, and stays that
    // way until a transcript says otherwise.
    for other in ["lp-emu:esp32c6:t1", "lp-emu:esp32c6:t2", "silicon:esp32c6"] {
        assert!(
            cfg.configuration(other)
                .unwrap()
                .trust
                .band(FieldClass::Timing)
                .is_none(),
            "{other} must state no band"
        );
    }

    // The grade and the band are separate claims: `--strict` still reads the
    // grade, and at `documented` it still refuses timing. Only
    // `--strict-timing` reads the band.
    assert!(
        t3.trust.grade(FieldClass::Timing) < Grade::Measured,
        "G1 has not been answered yet; the proposal is `documented`"
    );
}

// --- G4-3: the t3 replays -------------------------------------------------

#[test]
fn t3_is_within_band_on_shader_compile_stress() {
    let t3 = load(COMPILE, T3_COMPILE);
    let silicon = load(COMPILE, SILICON_COMPILE);
    let report = strict_timing(COMPILE, &t3, &silicon);

    let applied = report.timing_band.as_ref().expect("the t3 band applies");
    assert_eq!(applied.stated_by, "lp-emu:esp32c6:t3");
    assert!(
        applied.invert,
        "t3 is the left transcript, so the reference is the right one and the ratio is \
         right/left — the direction the calibration record's tables use"
    );

    let v = verdict(&report, "compile-tick", "slice_cycles");
    assert_eq!((v.in_band, v.samples), (85, 92), "calibration record §3");
    assert!((v.aggregate.unwrap() - 1.154).abs() < 0.001, "{v:?}");
    assert!(v.passed);

    assert!(report.is_ok(), "{:?}", report.failures());
    // The report still prints ratios for timing whether or not the flag is
    // on. A band changes the verdict, not the reading.
    assert!(
        report.render().contains("sum ratio 0.87x"),
        "{}",
        report.render()
    );
}

#[test]
fn t3_is_within_band_on_cycle_probe() {
    let t3 = load(PROBE, T3_PROBE);
    let silicon = load(PROBE, SILICON_PROBE);
    let report = strict_timing(PROBE, &t3, &silicon);

    let v = verdict(&report, "cycle-probe", "cycles");
    assert_eq!((v.in_band, v.samples), (75, 80));
    assert!((v.aggregate.unwrap() - 1.017).abs() < 0.001, "{v:?}");
    assert!(report.is_ok(), "{:?}", report.failures());
}

/// The null hypothesis, replayed rather than argued: the band the model
/// passes is one the *previous* grade fails, on both halves. A band that
/// every grade satisfies would be a mask with a decimal point.
#[test]
fn the_band_refuses_the_grade_it_replaced() {
    let cfg = lp_emu_validate::ValidateConfig::embedded();
    let band = cfg
        .configuration("lp-emu:esp32c6:t3")
        .unwrap()
        .trust
        .band(FieldClass::Timing)
        .unwrap()
        .clone();

    // t2 wearing t3's band: the same comparison, the same payload, the older
    // model. `lp-emu:esp32c6:t2` is a configuration `validate.toml` knows and
    // states no band for, so the sidecar's own entry is what is read.
    let t2 = load_with_trust(
        COMPILE,
        T2_COMPILE,
        TrustTable::new(vec![TrustEntry {
            class: FieldClass::Timing,
            grade: Grade::Modeled,
            band: Some(band),
            because: "the null hypothesis, for this test only".into(),
        }]),
    );
    let silicon = load(COMPILE, SILICON_COMPILE);
    let report = strict_timing(COMPILE, &t2, &silicon);

    let v = verdict(&report, "compile-tick", "slice_cycles");
    assert_eq!(
        (v.in_band, v.samples),
        (25, 92),
        "calibration record §3's null"
    );
    assert!((v.aggregate.unwrap() - 1.560).abs() < 0.001, "{v:?}");
    assert!(!v.passed);
    assert!(!report.is_ok());
    let failure = report
        .failures()
        .into_iter()
        .find(|f| f.contains("compile-tick.slice_cycles"))
        .expect("the band names the field it refused");
    assert!(
        failure.contains("27.2 %"),
        "the observed coverage: {failure}"
    );
    assert!(
        failure.contains("aggregate 1.560"),
        "the observed aggregate: {failure}"
    );
}

// --- G4-2: the three negative controls ------------------------------------

/// Move one figure far enough and the aggregate catches it, even though the
/// coverage test alone would not: 92 samples with a 90 % floor can lose nine
/// and still pass, which is exactly why the aggregate is the second half.
#[test]
fn one_slice_cycles_figure_far_outside_the_band_fails() {
    let perturbed = load_with_body(COMPILE, T3_COMPILE, |body| {
        body.replace(
            "tick=17 stage= slice_cycles=1375916",
            "tick=17 stage= slice_cycles=137591",
        )
    });
    let silicon = load(COMPILE, SILICON_COMPILE);
    let report = strict_timing(COMPILE, &perturbed, &silicon);

    let v = verdict(&report, "compile-tick", "slice_cycles");
    assert_eq!(v.in_band, 84, "one tick left the interval");
    assert!(
        v.coverage >= 0.90,
        "coverage alone still passes at {:.3} — the point of the control",
        v.coverage
    );
    assert!(
        v.aggregate.unwrap() > 1.20,
        "the aggregate is what refuses it: {:?}",
        v.aggregate
    );
    assert!(!v.passed);
    assert!(!report.is_ok());
    let failure = report
        .failures()
        .into_iter()
        .find(|f| f.contains("compile-tick.slice_cycles"))
        .expect("named");
    assert!(failure.contains("band allows 20 %"), "{failure}");
}

/// And move every figure, and both halves catch it.
#[test]
fn slice_cycles_moved_wholesale_fails_on_coverage_too() {
    let perturbed = load_with_body(COMPILE, T3_COMPILE, |body| {
        rewrite_slice_cycles(&body, |c| c * 2)
    });
    let silicon = load(COMPILE, SILICON_COMPILE);
    let report = strict_timing(COMPILE, &perturbed, &silicon);

    let v = verdict(&report, "compile-tick", "slice_cycles");
    assert_eq!(v.in_band, 0, "every ratio halved is outside [0.80, 1.25]");
    assert!(!v.passed);
    let failure = report
        .failures()
        .into_iter()
        .find(|f| f.contains("compile-tick.slice_cycles"))
        .expect("named");
    assert!(failure.contains("0/92 samples (0.0 %)"), "{failure}");
    assert!(failure.contains("band asks for 90 %"), "{failure}");
}

/// The other side of the same control: a figure moved a *little* passes,
/// which is the whole reason the band exists. Under the old exact rule this
/// replay failed 188 times over.
#[test]
fn slice_cycles_perturbed_inside_the_band_passes() {
    let perturbed = load_with_body(COMPILE, T3_COMPILE, |body| {
        // 3 % on every slice: nowhere near [0.80, 1.25]'s edges, and not one
        // figure left equal to what silicon recorded.
        rewrite_slice_cycles(&body, |c| c * 103 / 100)
    });
    let silicon = load(COMPILE, SILICON_COMPILE);
    let report = strict_timing(COMPILE, &perturbed, &silicon);

    let v = verdict(&report, "compile-tick", "slice_cycles");
    assert_eq!(v.samples, 92);
    assert_eq!(
        report
            .differences_in(FieldClass::Timing)
            .filter(|c| c.field == "slice_cycles")
            .count(),
        92,
        "all 92 differ — nothing here is passing by being equal"
    );
    assert!(v.passed, "{v:?}");
    assert!(report.is_ok(), "{:?}", report.failures());
}

/// A band is a statement about a class that is never exact. Memory is not
/// that class, and no band reaches it: the same perturbation to a heap figure
/// fails with the band in force and with `--strict-timing` off entirely.
#[test]
fn a_corrupted_memory_field_still_fails_regardless_of_any_band() {
    let corrupted = load_with_body(COMPILE, T3_COMPILE, |body| {
        body.replace(
            "tick=1 stage= slice_cycles=174385 slice_us=1089 mem_before=321600",
            "tick=1 stage= slice_cycles=174385 slice_us=1089 mem_before=321592",
        )
    });
    let silicon = load(COMPILE, SILICON_COMPILE);

    for options in [
        ReplayOptions::default(),
        ReplayOptions {
            strict: false,
            strict_timing: true,
        },
    ] {
        let report = replay(&corrupted, &silicon, options).unwrap();
        assert!(
            report.timing_band.is_some(),
            "the band is in force; it simply has nothing to say about memory"
        );
        let failure = report
            .failures()
            .into_iter()
            .find(|f| f.starts_with("memory field"))
            .unwrap_or_else(|| panic!("eight bytes of heap must fail: {:?}", report.failures()));
        assert!(failure.contains("321592 vs 321600"), "{failure}");
    }
}

// --- the `on` list is the boundary ----------------------------------------

/// A band does not travel to a payload nobody measured it on. Same band, same
/// transcripts, one word different in `on`, and the comparison goes back to
/// being exact — which is `modeled` behaviour, because that is what the
/// configuration is there.
#[test]
fn a_band_that_does_not_name_the_payload_does_not_reach_it() {
    let silicon = load(COMPILE, SILICON_COMPILE);
    let band_over = |on: &[&str]| Band {
        per_sample: [0.80, 1.25],
        per_sample_coverage: 0.90,
        aggregate: 0.20,
        on: on.iter().map(|s| (*s).to_string()).collect(),
    };
    let trust = |on: &[&str]| {
        TrustTable::new(vec![TrustEntry {
            class: FieldClass::Timing,
            grade: Grade::Modeled,
            band: Some(band_over(on)),
            because: "for this test only".into(),
        }])
    };

    // `lp-emu:esp32c6:t2` states no band in validate.toml, so its sidecar's
    // entry is what is read — which is also the fallback path's own test.
    let names_it = load_with_trust(COMPILE, T2_COMPILE, trust(&[COMPILE]));
    let report = strict_timing(COMPILE, &names_it, &silicon);
    assert!(report.timing_band.is_some(), "the band names this payload");

    let names_another = load_with_trust(COMPILE, T2_COMPILE, trust(&[PROBE]));
    let report = strict_timing(COMPILE, &names_another, &silicon);
    assert!(
        report.timing_band.is_none(),
        "a band measured on cycle-probe says nothing about shader-compile-stress"
    );
    // And with no band in force, the pre-band rule is what runs: exact.
    assert_eq!(
        report.failures().len(),
        report.differences_in(FieldClass::Timing).count()
    );
    assert!(!report.is_ok());
}

/// The ratio is read reference-over-model, so a band means the same thing
/// whichever way round the two transcripts were handed to `replay`. A
/// contract that depends on argument order is not a contract.
#[test]
fn the_band_reads_the_same_ratio_whichever_argument_is_first() {
    let t3 = load(COMPILE, T3_COMPILE);
    let silicon = load(COMPILE, SILICON_COMPILE);

    let a = strict_timing(COMPILE, &t3, &silicon);
    let b = strict_timing(COMPILE, &silicon, &t3);

    let (va, vb) = (
        verdict(&a, "compile-tick", "slice_cycles"),
        verdict(&b, "compile-tick", "slice_cycles"),
    );
    assert_eq!((va.in_band, va.samples), (vb.in_band, vb.samples));
    assert!((va.aggregate.unwrap() - vb.aggregate.unwrap()).abs() < 1e-9);
    assert!(a.timing_band.as_ref().unwrap().invert);
    assert!(!b.timing_band.as_ref().unwrap().invert);
    assert!(a.is_ok() && b.is_ok());
}

// --- G4-6: RD4, the pinned image ------------------------------------------

/// The grade-3 gates replay the image the calibration record measured, and
/// not whatever a PR happened to build.
///
/// Nothing here builds firmware — the shas are read out of the committed
/// sidecars — which is the point: the rule costs no wall time and still makes
/// a swapped transcript loud. The one gap is named rather than papered over:
/// `shader-compile-stress`'s silicon capture predates `firmware_sha256`
/// (it was recorded 2026-09-07, and the field is additive), so its side is
/// pinned by commit alone.
#[test]
fn the_grade_3_replays_run_against_the_pinned_images() {
    let cases: [(&str, &str, Option<&str>, &str); 4] = [
        (COMPILE, T3_COMPILE, Some(SHA_T3_COMPILE), "17ac011f7"),
        (PROBE, T3_PROBE, Some(SHA_T3_PROBE), "17ac011f7"),
        (
            PROBE,
            SILICON_PROBE,
            Some(SHA_SILICON_PROBE),
            "b89893962c76",
        ),
        (COMPILE, SILICON_COMPILE, None, "735af98ae9d9"),
    ];
    for (payload, name, sha, commit) in cases {
        let t = load(payload, name);
        assert_eq!(t.header.firmware_commit, commit, "{name}");
        match sha {
            Some(want) => assert_eq!(
                t.header.firmware_sha256.as_deref(),
                Some(want),
                "{name} was recorded from different bytes than the record names — a \
                 re-recording is a new stem with its own sha, never a swap"
            ),
            None => assert!(
                t.header.firmware_sha256.is_none(),
                "{name} has gained a sha; pin it here rather than leaving it unasserted"
            ),
        }
    }
}

/// Rewrite every `slice_cycles=` figure in a transcript body.
fn rewrite_slice_cycles(body: &str, f: impl Fn(u64) -> u64) -> String {
    body.lines()
        .map(|line| match line.split_once("slice_cycles=") {
            None => line.to_string(),
            Some((head, tail)) => {
                let end = tail.find(' ').unwrap_or(tail.len());
                let n: u64 = tail[..end].parse().unwrap();
                format!("{head}slice_cycles={}{}", f(n), &tail[end..])
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
