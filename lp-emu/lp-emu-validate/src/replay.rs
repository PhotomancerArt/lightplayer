//! Replay: one transcript against another, field by field.
//!
//! The verdict is structured, not textual. Two transcripts of the same payload
//! are compared record field by record field and series sample by series
//! sample, each comparison carrying the field class it belongs to. Then:
//!
//! * a difference in a **memory**, **pin**, **wire**, **usb-serial-jtag** or
//!   **structural** field fails the replay. These are the claims a transcript
//!   exists to make.
//! * a difference in a **timing** or **boot-log** field is *reported with its
//!   ratio*, not failed. Time is where a configuration is allowed to be wrong,
//!   and hiding that would be the mistake — the spike found the harness's own
//!   5 ms slice budget passing under esp-emu and failing on silicon.
//! * `--strict` additionally refuses any class where either configuration's
//!   trust table grades the claim below `measured`.
//!
//! `--strict-timing` compared timing fields for **equality** until M1 P4, and
//! a cycle model is never equal. A trust entry may now carry a [`Band`] — a
//! stated interval, a coverage fraction and an aggregate tolerance, naming the
//! payloads that measured it — and where one side of the replay carries one
//! for the payload in hand, `--strict-timing` enforces *within-band* instead.
//! Where neither side does, it is the same exact comparison it always was.
//! The report's ratios are printed either way; the band changes what fails,
//! never what is shown.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use anyhow::{Result, bail};

use crate::configuration::Band;
use crate::grade::{FieldClass, Grade};
use crate::payload::PinCapture;
use crate::transcript::Transcript;

#[derive(Clone, Copy, Debug, Default)]
pub struct ReplayOptions {
    /// Refuse a class whose trust grade is below `Measured` on either side.
    pub strict: bool,
    /// Treat timing divergence as a failure. Off by default, and it should
    /// stay off for cross-configuration replays: PD9 says no host gate runs on
    /// emulated microseconds.
    pub strict_timing: bool,
}

/// One field, on both sides.
#[derive(Clone, Debug)]
pub struct FieldCompare {
    /// `case-summary` or `compile-tick[17]`.
    pub scope: String,
    pub field: String,
    pub class: FieldClass,
    pub left: String,
    pub right: String,
    pub equal: bool,
    /// `left / right`, when both parse as numbers and the right is non-zero.
    pub ratio: Option<f64>,
    /// True when this came from a line series rather than a structured record.
    /// Series comparisons are aggregated in the rendered report — 92 ticks of
    /// per-field detail is a wall, and the sums are the readable claim.
    pub from_series: bool,
}

/// A series' aggregate, per field.
#[derive(Clone, Debug)]
pub struct SeriesFieldSummary {
    pub series: &'static str,
    pub field: String,
    pub class: FieldClass,
    pub samples: usize,
    pub equal: usize,
    pub sum_left: Option<f64>,
    pub sum_right: Option<f64>,
    /// `sum_left / sum_right`.
    pub ratio: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct ReplayReport {
    pub payload: &'static str,
    pub left_name: String,
    pub right_name: String,
    pub left_config: String,
    pub right_config: String,
    pub mask_set: &'static str,
    pub comparisons: Vec<FieldCompare>,
    pub series_summaries: Vec<SeriesFieldSummary>,
    /// Shapes that did not line up at all: a missing record kind, a different
    /// number of ticks, a sentinel that never arrived.
    pub structural_problems: Vec<String>,
    /// Strict-mode refusals.
    pub grade_problems: Vec<String>,
    /// Why the decoded **pad** was not compared, when the payload declares a
    /// pin capture and a configuration on one side or the other cannot produce
    /// one. `None` means the question did not arise: either the payload makes
    /// no pin claim, or both sides record pins and the pad was compared like
    /// anything else. Never a silent skip — the report says it.
    ///
    /// Not the same as the `pin` field CLASS, which may be compared on the
    /// same run: `ws281x-telemetry`'s trips, skips and errors are pin-class
    /// claims the guest makes about its own driver and they arrive in the
    /// console. This is about the pad itself.
    pub pin_not_compared: Option<String>,
    /// The timing band in force for this replay, if either side stated one for
    /// this payload. `None` means `--strict-timing` compares exactly, as it
    /// did before bands existed.
    pub timing_band: Option<AppliedBand>,
    pub options: ReplayOptions,
}

/// A band, resolved against this particular pair of transcripts.
#[derive(Clone, Debug)]
pub struct AppliedBand {
    pub band: Band,
    /// The configuration whose trust entry stated it.
    pub stated_by: String,
    /// True when the banded side is the **left** transcript, so the canonical
    /// reference-over-model ratio is `right / left` — the reciprocal of the
    /// `left / right` the report prints. See [`Band`]'s "Direction".
    pub invert: bool,
}

impl AppliedBand {
    /// `FieldCompare::ratio` is `left / right`; the band is read
    /// reference-over-model. Turn one into the other.
    fn canonical(&self, ratio: f64) -> Option<f64> {
        if self.invert {
            (ratio != 0.0).then(|| 1.0 / ratio)
        } else {
            Some(ratio)
        }
    }

    fn direction(&self) -> &'static str {
        if self.invert { "right/left" } else { "left/right" }
    }
}

/// How one timing field fared against the band.
#[derive(Clone, Debug)]
pub struct BandVerdict {
    /// `compile-tick.slice_cycles`.
    pub scope: String,
    pub field: String,
    pub samples: usize,
    pub in_band: usize,
    /// `in_band / samples`.
    pub coverage: f64,
    /// `sum(reference) / sum(model)`, in the band's own direction.
    pub aggregate: Option<f64>,
    /// Samples whose value did not parse as a number on one side or the other:
    /// a band cannot speak for those, so they are still compared exactly.
    pub unbandable_differences: usize,
    pub passed: bool,
}

/// Classes whose divergence fails a replay.
const HARD_CLASSES: &[FieldClass] = &[
    FieldClass::Memory,
    FieldClass::Pin,
    FieldClass::Wire,
    FieldClass::UsbSerialJtag,
    FieldClass::Structural,
];

impl ReplayReport {
    pub fn comparisons_in(&self, class: FieldClass) -> impl Iterator<Item = &FieldCompare> {
        self.comparisons.iter().filter(move |c| c.class == class)
    }

    pub fn differences_in(&self, class: FieldClass) -> impl Iterator<Item = &FieldCompare> {
        self.comparisons_in(class).filter(|c| !c.equal)
    }

    pub fn compared(&self, class: FieldClass) -> usize {
        self.comparisons_in(class).count()
    }

    /// Classes that appear in this replay.
    pub fn classes(&self) -> Vec<FieldClass> {
        let set: BTreeSet<_> = self.comparisons.iter().map(|c| c.class).collect();
        set.into_iter().collect()
    }

    pub fn failures(&self) -> Vec<String> {
        let mut out = self.structural_problems.clone();
        out.extend(self.grade_problems.iter().cloned());
        for class in HARD_CLASSES {
            for c in self.differences_in(*class) {
                out.push(format!(
                    "{class} field {}.{} differs: {} vs {}",
                    c.scope, c.field, c.left, c.right
                ));
            }
        }
        if self.options.strict_timing {
            match &self.timing_band {
                // No band on either side, or the band does not speak for this
                // payload: exactly the comparison this has always been.
                None => {
                    for c in self.differences_in(FieldClass::Timing) {
                        out.push(format!(
                            "timing field {}.{} differs: {} vs {} (--strict-timing)",
                            c.scope, c.field, c.left, c.right
                        ));
                    }
                }
                Some(applied) => {
                    for v in self.band_verdicts() {
                        if v.passed {
                            continue;
                        }
                        let mut why = Vec::new();
                        if v.coverage < applied.band.per_sample_coverage {
                            why.push(format!(
                                "{}/{} samples ({:.1} %) inside [{:.2}, {:.2}], band asks for \
                                 {:.0} %",
                                v.in_band,
                                v.samples,
                                v.coverage * 100.0,
                                applied.band.per_sample[0],
                                applied.band.per_sample[1],
                                applied.band.per_sample_coverage * 100.0
                            ));
                        }
                        match v.aggregate {
                            Some(a) if (a - 1.0).abs() > applied.band.aggregate => {
                                why.push(format!(
                                    "aggregate {a:.3} is {:.1} % out, band allows {:.0} %",
                                    (a - 1.0).abs() * 100.0,
                                    applied.band.aggregate * 100.0
                                ));
                            }
                            None => why.push("no aggregate could be computed".into()),
                            Some(_) => {}
                        }
                        if v.unbandable_differences > 0 {
                            why.push(format!(
                                "{} sample(s) differ and are not numbers, so no band speaks for \
                                 them",
                                v.unbandable_differences
                            ));
                        }
                        out.push(format!(
                            "timing field {}.{} is outside the band `{}` stated by `{}` \
                             (ratios read {}): {} (--strict-timing)",
                            v.scope,
                            v.field,
                            applied.band.describe(),
                            applied.stated_by,
                            applied.direction(),
                            why.join("; ")
                        ));
                    }
                }
            }
        }
        out
    }

    /// How each timing field fared against the band in force.
    ///
    /// Empty when no band is in force. Coverage is per **field**, over all of
    /// that field's samples in this replay: `compile-tick[1..92].slice_cycles`
    /// is one verdict over 92 samples, and a record field that appears once is
    /// one verdict over one sample (which must then be in band, since no
    /// coverage fraction of one sample is less than one).
    pub fn band_verdicts(&self) -> Vec<BandVerdict> {
        let Some(applied) = &self.timing_band else {
            return Vec::new();
        };
        // Series scopes are `compile-tick[17]`; the samples of one field
        // belong together, so the index is not part of the key.
        let family = |scope: &str| -> String {
            scope
                .split_once('[')
                .map_or_else(|| scope.to_string(), |(head, _)| head.to_string())
        };
        let mut groups: BTreeMap<(String, String), Vec<&FieldCompare>> = BTreeMap::new();
        for c in self.comparisons_in(FieldClass::Timing) {
            groups
                .entry((family(&c.scope), c.field.clone()))
                .or_default()
                .push(c);
        }
        let mut out = Vec::new();
        for ((scope, field), cs) in groups {
            let mut samples = 0usize;
            let mut in_band = 0usize;
            let mut sum_ref = 0.0f64;
            let mut sum_model = 0.0f64;
            let mut sums_ok = true;
            let mut unbandable = 0usize;
            for c in &cs {
                let Some(ratio) = c.ratio.and_then(|r| applied.canonical(r)) else {
                    // Not a number on one side, or a zero denominator. A band
                    // is a statement about magnitudes and has nothing to say
                    // here, so the old rule stands: it has to be equal.
                    if !c.equal {
                        unbandable += 1;
                    }
                    continue;
                };
                samples += 1;
                if applied.band.contains(ratio) {
                    in_band += 1;
                }
                match (c.left.parse::<f64>(), c.right.parse::<f64>()) {
                    (Ok(l), Ok(r)) => {
                        let (reference, model) = if applied.invert { (r, l) } else { (l, r) };
                        sum_ref += reference;
                        sum_model += model;
                    }
                    _ => sums_ok = false,
                }
            }
            if samples == 0 && unbandable == 0 {
                continue;
            }
            let coverage = if samples == 0 {
                0.0
            } else {
                in_band as f64 / samples as f64
            };
            let aggregate = (sums_ok && sum_model != 0.0).then_some(sum_ref / sum_model);
            let passed = unbandable == 0
                && samples > 0
                && coverage >= applied.band.per_sample_coverage
                && aggregate.is_some_and(|a| (a - 1.0).abs() <= applied.band.aggregate);
            out.push(BandVerdict {
                scope,
                field,
                samples,
                in_band,
                coverage,
                aggregate,
                unbandable_differences: unbandable,
                passed,
            });
        }
        out
    }

    pub fn is_ok(&self) -> bool {
        self.failures().is_empty()
    }

    pub fn render(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(
            s,
            "payload {} — {} vs {}",
            self.payload, self.left_config, self.right_config
        );
        let _ = writeln!(s, "  left  {}", self.left_name);
        let _ = writeln!(s, "  right {}", self.right_name);
        let _ = writeln!(s, "  mask set: {}", self.mask_set);
        let _ = writeln!(s);

        let _ = writeln!(
            s,
            "  {:<16} {:>9} {:>9} {:>9}",
            "class", "compared", "equal", "differ"
        );
        for class in self.classes() {
            let compared = self.compared(class);
            let differ = self.differences_in(class).count();
            let _ = writeln!(
                s,
                "  {:<16} {:>9} {:>9} {:>9}",
                class.slug(),
                compared,
                compared - differ,
                differ
            );
        }
        if let Some(why) = &self.pin_not_compared {
            // A row, not a footnote: a reader scanning the table has to see
            // this in the table. Labelled `pin capture` rather than `pin`
            // because the CLASS may well have been compared on the line above
            // — `ws281x-telemetry`'s trips, skips and errors are pin-class
            // claims the guest makes about its own driver, and they travel in
            // the console. What is missing is the decoded PAD, which is a
            // different reading of the same pin and the only one an
            // instrument could confirm.
            let _ = writeln!(s, "  {:<16} {:>9}   — {why}", "pin capture", "not compared");
        }

        // The band, and how each timing field sat in it. Printed whether or
        // not `--strict-timing` is on: what the band says is a fact about this
        // pair of transcripts, and a reader deciding whether to trust the
        // timing column should see it without having to ask for a gate. The
        // ratios below are unchanged either way (PD9/D13: a band is not a
        // licence to gate a product number on emulated microseconds).
        if let Some(applied) = &self.timing_band {
            let _ = writeln!(
                s,
                "\n  timing band {} — stated by {}, ratios read {} (reference / model):",
                applied.band.describe(),
                applied.stated_by,
                applied.direction()
            );
            for v in self.band_verdicts() {
                let agg = v
                    .aggregate
                    .map_or_else(|| "—".to_string(), |a| format!("{a:.3}"));
                let _ = writeln!(
                    s,
                    "    {:<34} {:>3}/{:<3} in band ({:>5.1} %), aggregate {agg}  {}",
                    format!("{}.{}", v.scope, v.field),
                    v.in_band,
                    v.samples,
                    v.coverage * 100.0,
                    if v.passed { "within band" } else { "OUTSIDE" }
                );
            }
        }

        let timing: Vec<_> = self
            .differences_in(FieldClass::Timing)
            .filter(|c| !c.from_series)
            .collect();
        let series_timing = self
            .differences_in(FieldClass::Timing)
            .filter(|c| c.from_series)
            .count();
        if !timing.is_empty() {
            let _ = writeln!(s, "\n  timing divergence (left / right):");
            for c in timing {
                match c.ratio {
                    Some(r) => {
                        let _ = writeln!(
                            s,
                            "    {:<28} {:>12} {:>12}  {r:.2}x",
                            format!("{}.{}", c.scope, c.field),
                            c.left,
                            c.right
                        );
                    }
                    None => {
                        let _ = writeln!(
                            s,
                            "    {:<28} {:>12} {:>12}",
                            format!("{}.{}", c.scope, c.field),
                            c.left,
                            c.right
                        );
                    }
                }
            }
        }

        if series_timing > 0 {
            let _ = writeln!(
                s,
                "    (+ {series_timing} per-sample timing differences in the series below)"
            );
        }

        if !self.series_summaries.is_empty() {
            let _ = writeln!(s, "\n  series:");
            for sum in &self.series_summaries {
                let ratio = sum
                    .ratio
                    .map(|r| format!("{r:.2}x"))
                    .unwrap_or_else(|| "-".into());
                let _ = writeln!(
                    s,
                    "    {:<34} {:>5} samples, {:>5} equal, sum ratio {ratio}",
                    format!("{}.{} [{}]", sum.series, sum.field, sum.class.slug()),
                    sum.samples,
                    sum.equal
                );
            }
        }

        let failures = self.failures();
        let _ = writeln!(s);
        if failures.is_empty() {
            let _ = writeln!(s, "  REPLAY OK");
        } else {
            let _ = writeln!(s, "  REPLAY FAILED ({} problem(s)):", failures.len());
            for f in failures.iter().take(20) {
                let _ = writeln!(s, "    {f}");
            }
            if failures.len() > 20 {
                let _ = writeln!(s, "    … and {} more", failures.len() - 20);
            }
        }
        s
    }
}

/// Compare two transcripts of the same payload.
pub fn replay(
    left: &Transcript,
    right: &Transcript,
    options: ReplayOptions,
) -> Result<ReplayReport> {
    if !std::ptr::eq(left.payload, right.payload) {
        bail!(
            "cannot replay `{}` against `{}`: different payloads",
            left.payload.name,
            right.payload.name
        );
    }
    if left.header.chip != right.header.chip {
        bail!(
            "cannot replay across chips: `{}` vs `{}`",
            left.header.chip,
            right.header.chip
        );
    }

    let payload = left.payload;
    let mut comparisons = Vec::new();
    let mut structural_problems = Vec::new();

    // The sentinel: did the payload actually get where it was going, on both
    // sides? Comparing numbers from a run that never finished is worse than
    // comparing nothing.
    for (t, side) in [(left, "left"), (right, "right")] {
        if t.sentinel_line().is_none() {
            structural_problems.push(format!(
                "{side} transcript never reached the payload sentinel `{}`",
                payload.sentinel.marker()
            ));
        }
    }

    // --- structured records ------------------------------------------------
    let left_records = left.records()?;
    let right_records = right.records()?;
    for kind in payload.record_kinds {
        let ls: Vec<_> = left_records.iter().filter(|r| &r.kind == kind).collect();
        let rs: Vec<_> = right_records.iter().filter(|r| &r.kind == kind).collect();
        if ls.len() != rs.len() {
            structural_problems.push(format!(
                "record kind `{kind}`: {} on the left, {} on the right",
                ls.len(),
                rs.len()
            ));
            continue;
        }
        if ls.is_empty() {
            structural_problems.push(format!(
                "record kind `{kind}` is declared by payload `{}` but appears in neither transcript",
                payload.name
            ));
            continue;
        }
        for (n, (l, r)) in ls.iter().zip(rs.iter()).enumerate() {
            let scope = if ls.len() == 1 {
                (*kind).to_string()
            } else {
                format!("{kind}[{n}]")
            };
            let fields: BTreeSet<&String> = l.fields.keys().chain(r.fields.keys()).collect();
            for field in fields {
                if field == "kind" {
                    continue;
                }
                let Some(class) = payload.class_of(kind, field) else {
                    // A field the registry does not classify is a field nobody
                    // decided the meaning of. Say so rather than guessing.
                    structural_problems.push(format!(
                        "record `{kind}` carries field `{field}`, which payload `{}` does not \
                         classify — add a FieldSpec for it",
                        payload.name
                    ));
                    continue;
                };
                let lv = l.get(field).map(render_json).unwrap_or_default();
                let rv = r.get(field).map(render_json).unwrap_or_default();
                comparisons.push(compare(&scope, field, class, lv, rv, false));
            }
        }
    }

    // --- line series -------------------------------------------------------
    let mut series_summaries = Vec::new();
    for spec in payload.series {
        let ls = index_series(left, spec);
        let rs = index_series(right, spec);
        let keys: BTreeSet<&String> = ls.keys().chain(rs.keys()).collect();
        if ls.len() != rs.len() {
            structural_problems.push(format!(
                "series `{}`: {} samples on the left, {} on the right",
                spec.name,
                ls.len(),
                rs.len()
            ));
        }
        let mut acc: BTreeMap<&str, (usize, usize, Option<f64>, Option<f64>)> = BTreeMap::new();
        for key in keys {
            let (Some(l), Some(r)) = (ls.get(key), rs.get(key)) else {
                structural_problems.push(format!(
                    "series `{}` sample `{key}` appears on only one side",
                    spec.name
                ));
                continue;
            };
            for (field, class) in spec.fields {
                let lv = l.get(*field).cloned().unwrap_or_default();
                let rv = r.get(*field).cloned().unwrap_or_default();
                let c = compare(
                    &format!("{}[{key}]", spec.name),
                    field,
                    *class,
                    lv,
                    rv,
                    true,
                );
                let entry = acc.entry(field).or_insert((0, 0, Some(0.0), Some(0.0)));
                entry.0 += 1;
                entry.1 += usize::from(c.equal);
                accumulate(&mut entry.2, &c.left);
                accumulate(&mut entry.3, &c.right);
                comparisons.push(c);
            }
        }
        for (field, class) in spec.fields {
            if let Some((samples, equal, sum_left, sum_right)) = acc.get(field) {
                let ratio = match (sum_left, sum_right) {
                    (Some(l), Some(r)) if *r != 0.0 => Some(l / r),
                    _ => None,
                };
                series_summaries.push(SeriesFieldSummary {
                    series: spec.name,
                    field: (*field).to_string(),
                    class: *class,
                    samples: *samples,
                    equal: *equal,
                    sum_left: *sum_left,
                    sum_right: *sum_right,
                    ratio,
                });
            }
        }
    }

    // --- the pin capture ---------------------------------------------------
    //
    // The half of a recording that is not something the device said. Two
    // claims are checked, and they are different claims:
    //
    //  1. **Within** each transcript, the guest's own checksum against the
    //     bytes the pad carried, frame by frame. A disagreement there means
    //     the driver and the wire disagree on one machine, which is not a
    //     difference between configurations at all — it is a structural
    //     problem in that recording, and it is named as one.
    //  2. **Between** the two, the pad's own bytes per frame, as a `Pin`
    //     comparison. That is the claim the class exists for and the one
    //     that fails a replay.
    //
    // Whether a side can produce a pin log at all is a property of its
    // CONFIGURATION, not of the payload: an `lp-emu:*` machine decodes the pad
    // off its own signal fabric and silicon cannot, because reading a real pad
    // needs an instrument nobody has put on this bench. Asking silicon for one
    // made the first silicon capture of `rmt-chase` fail with 3,073 equal
    // structural comparisons underneath the red
    // (`docs/defects/2026-09-08-a-pin-capture-is-a-property-of-the-configuration-not-the-payload.md`).
    //
    // Stated in `validate.toml`, never inferred from the configuration's name.
    let cfg = crate::config::ValidateConfig::embedded();
    let records_pins = |t: &Transcript| -> Result<bool> {
        Ok(cfg.configuration(&t.header.configuration)?.records_pins)
    };
    let (left_pins, right_pins) = (records_pins(left)?, records_pins(right)?);
    // Said in the report rather than silently skipped: a replay that cannot
    // compare pins must not read as one that compared them and agreed.
    let pin_note = match (payload.pin_capture.is_on(), left_pins, right_pins) {
        (false, _, _) => None,
        (true, true, true) => None,
        (true, false, false) => Some(format!(
            "neither {} nor {} records pins",
            left.header.configuration, right.header.configuration
        )),
        (true, false, true) => Some(format!("{} records none", left.header.configuration)),
        (true, true, false) => Some(format!("{} records none", right.header.configuration)),
    };

    if payload.pin_capture.is_on() && left_pins && right_pins {
        for (t, side) in [(left, "left"), (right, "right")] {
            for problem in pin_self_disagreements(t, side)? {
                structural_problems.push(problem);
            }
        }
        let (lp, rp) = (left.pin_records()?, right.pin_records()?);
        match payload.pin_capture {
            PinCapture::EveryFrame if lp.len() != rp.len() => {
                structural_problems.push(format!(
                    "pin capture: {} decoded frames on the left, {} on the right",
                    lp.len(),
                    rp.len()
                ));
            }
            // A shipped-image walk's pad runs on at the engine's pace until
            // the run ends on a console line, so the count is the clock's:
            // reported with its ratio, like every other timing figure, while
            // the frames the two share are compared below as `Pin`.
            PinCapture::WhileRunning => comparisons.push(compare(
                "pin",
                "frames",
                FieldClass::Timing,
                lp.len().to_string(),
                rp.len().to_string(),
                false,
            )),
            PinCapture::EveryFrame | PinCapture::Off => {}
        }
        for (n, (l, r)) in lp.iter().zip(rp.iter()).enumerate() {
            let scope = format!("pin[{n}]");
            for field in ["pad", "signal", "n", "leds", "bits", "complete"] {
                comparisons.push(compare(
                    &scope,
                    field,
                    FieldClass::Pin,
                    l.get(field).map(render_json).unwrap_or_default(),
                    r.get(field).map(render_json).unwrap_or_default(),
                    true,
                ));
            }
            // The bytes themselves, as a checksum: a per-frame hex string of
            // 768 bytes in a failure message helps nobody, and the checksum
            // is the same function the guest's record uses.
            comparisons.push(compare(
                &scope,
                "wire_crc",
                FieldClass::Pin,
                pin_wire_crc(l),
                pin_wire_crc(r),
                true,
            ));
        }
    }

    // --- strict mode -------------------------------------------------------
    let mut grade_problems = Vec::new();
    if options.strict {
        let classes: BTreeSet<FieldClass> = comparisons.iter().map(|c| c.class).collect();
        for class in classes {
            if class == FieldClass::Structural {
                continue;
            }
            for (t, side) in [(left, "left"), (right, "right")] {
                let grade = t.header.trust.grade(class);
                if grade < Grade::Measured {
                    grade_problems.push(format!(
                        "strict: {side} configuration `{}` is graded `{grade}` for {class}{}",
                        t.header.configuration,
                        t.header
                            .trust
                            .because(class)
                            .map(|w| format!(" ({w})"))
                            .unwrap_or_else(|| " (no trust entry at all)".into())
                    ));
                }
            }
        }
    }

    // --- the timing band ---------------------------------------------------
    //
    // Resolved here rather than in `failures()` so the report carries what it
    // was judged against, and so the ratio's direction is decided once. The
    // band belongs to the configuration that stated it, and the *other* side
    // is the reference: silicon / t3, the direction the calibration record's
    // tables use. A band that named no payloads was refused at parse time; a
    // band that does not name THIS payload does not travel to it.
    fn band_of<'a>(t: &'a Transcript, payload: &str) -> Option<&'a Band> {
        t.header
            .trust
            .band(FieldClass::Timing)
            .filter(|b| b.covers(payload))
    }
    let (left_band, right_band) = (band_of(left, payload.name), band_of(right, payload.name));
    let timing_band = match (left_band, right_band) {
        (None, None) => None,
        // Both sides state one: there is no reference side to divide by, so
        // the printed direction is kept and the left's band is enforced. No
        // configuration pair in the tree does this today.
        (Some(b), Some(_)) => Some(AppliedBand {
            band: b.clone(),
            stated_by: left.header.configuration.clone(),
            invert: false,
        }),
        (Some(b), None) => Some(AppliedBand {
            band: b.clone(),
            stated_by: left.header.configuration.clone(),
            invert: true,
        }),
        (None, Some(b)) => Some(AppliedBand {
            band: b.clone(),
            stated_by: right.header.configuration.clone(),
            invert: false,
        }),
    };

    Ok(ReplayReport {
        payload: payload.name,
        left_name: describe(left),
        right_name: describe(right),
        left_config: left.header.configuration.clone(),
        right_config: right.header.configuration.clone(),
        mask_set: payload.mask_set,
        comparisons,
        series_summaries,
        structural_problems,
        grade_problems,
        pin_not_compared: pin_note,
        timing_band,
        options,
    })
}

fn describe(t: &Transcript) -> String {
    t.path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| format!("<{}>", t.header.configuration))
}

fn index_series(
    t: &Transcript,
    spec: &crate::payload::SeriesSpec,
) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for sample in t.series(spec) {
        out.insert(sample.key, sample.values);
    }
    out
}

fn compare(
    scope: &str,
    field: &str,
    class: FieldClass,
    left: String,
    right: String,
    from_series: bool,
) -> FieldCompare {
    let equal = left == right;
    let ratio = match (left.parse::<f64>(), right.parse::<f64>()) {
        (Ok(l), Ok(r)) if r != 0.0 => Some(l / r),
        _ => None,
    };
    FieldCompare {
        scope: scope.to_string(),
        field: field.to_string(),
        class,
        left,
        right,
        equal,
        ratio,
        from_series,
    }
}

fn accumulate(slot: &mut Option<f64>, value: &str) {
    match (slot.as_mut(), value.parse::<f64>()) {
        (Some(acc), Ok(v)) => *acc += v,
        (Some(_), Err(_)) => *slot = None,
        (None, _) => {}
    }
}

/// FNV-1a, 32-bit, over a pin record's `wire` hex.
///
/// The sixth transcription of these two constants in the repository, and the
/// point of it is that this side must not be able to agree with the guest by
/// sharing its code: the firmware computes the same function over the bytes
/// it *handed the driver*, this computes it over the bytes the pad *carried*,
/// and the two agreeing is the claim.
///
/// `wire`, not `rgb`, and that is a decision the harness forces. `rgb` is the
/// wire bytes unpermuted by the configured colour order; but the harness's
/// `LedChannel` already swaps RGB into GRB before `lp-ws281x` permutes again,
/// so the bytes on the wire *are* the frame the payload checksummed and
/// `rgb` is that frame swapped once more. Comparing against `rgb` would
/// therefore fail on any frame that is not grey (DD34 d, the double colour
/// swap — filed, not fixed).
fn pin_wire_crc(record: &crate::transcript::Record) -> String {
    let Some(hex) = record.get("wire").and_then(serde_json::Value::as_str) else {
        return String::new();
    };
    let mut hash = 0x811c_9dc5u32;
    for pair in hex.as_bytes().chunks_exact(2) {
        let Ok(byte) = u8::from_str_radix(std::str::from_utf8(pair).unwrap_or("zz"), 16) else {
            return format!("<not hex: {hex}>");
        };
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("0x{hash:08x}")
}

/// Does this transcript's guest agree with its own pin capture, frame by
/// frame? A disagreement is a problem with **this** recording, not a
/// difference between two of them.
fn pin_self_disagreements(t: &Transcript, side: &str) -> Result<Vec<String>> {
    let pins = t.pin_records()?;
    if pins.is_empty() {
        return Ok(vec![format!(
            "{side} transcript's payload `{}` claims a pin capture and has none",
            t.payload.name
        )]);
    }
    // A payload with no per-frame claim of its own — a walk on the shipped
    // image, which prints no record per frame — has nothing here to disagree
    // with: its pad stands alone, and what it is compared against is the
    // other transcript (and, for `shader-oracle-walk`, the host oracle in
    // `tests/m5_replays.rs`).
    if !t.payload.record_kinds.contains(&"rmt-frame") {
        return Ok(Vec::new());
    }
    let claims = t.records()?;
    let claims: Vec<_> = claims.iter().filter(|r| r.kind == "rmt-frame").collect();
    let mut out = Vec::new();
    if claims.len() != pins.len() {
        out.push(format!(
            "{side}: the guest recorded {} frames and the pad carried {}",
            claims.len(),
            pins.len()
        ));
    }
    for (claim, pin) in claims.iter().zip(pins.iter()) {
        let claimed = claim.get("crc").map(render_json).unwrap_or_default();
        let observed = pin_wire_crc(pin);
        if claimed != observed {
            out.push(format!(
                "{side}: frame {} — the guest claims {claimed}, the pad carried {observed}",
                claim.get("n").map(render_json).unwrap_or_default()
            ));
            if out.len() > 8 {
                out.push(format!("{side}: … and more"));
                break;
            }
        }
    }
    Ok(out)
}

fn render_json(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
