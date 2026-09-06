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

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use anyhow::{Result, bail};

use crate::grade::{FieldClass, Grade};
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
    pub options: ReplayOptions,
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
            for c in self.differences_in(FieldClass::Timing) {
                out.push(format!(
                    "timing field {}.{} differs: {} vs {} (--strict-timing)",
                    c.scope, c.field, c.left, c.right
                ));
            }
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

fn render_json(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
