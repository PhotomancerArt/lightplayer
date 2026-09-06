//! The payload registry: what a transcript is a transcript *of*.
//!
//! A payload is a module in `fw-checks` behind a cargo feature (vision D14),
//! runnable many per image. This registry is the host's half: the payload's
//! name, the firmware feature that builds it, the marker that says it arrived,
//! and — the part that makes replay possible — which fields in its output mean
//! what.
//!
//! **This registry mirrors `fw-checks`, it does not import it.** `fw-checks` is
//! AGPL and lives outside the `lp-emu/` fence; importing it would breach
//! `just lint-emu-fence`. `lp-cli` depends on both and owns the parity test
//! (`lp-cli/tests/validate_registry_parity.rs`), so the duplication cannot
//! drift silently.

use std::sync::OnceLock;

use anyhow::{Result, bail};
use regex::Regex;

use crate::grade::FieldClass;

/// The line that says the payload got where it was going.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sentinel {
    /// The payload runs to completion and prints this.
    Done(&'static str),
    /// The payload announces readiness and then serves until told to stop.
    /// Host-driven payloads (the GPIO calibration protocol) work this way.
    Ready(&'static str),
}

impl Sentinel {
    pub const fn marker(self) -> &'static str {
        match self {
            Self::Done(m) | Self::Ready(m) => m,
        }
    }

    /// The `done_marker` a matching `fw-checks` registry entry must declare.
    /// `Ready` payloads never finish, so theirs is `None`.
    pub const fn fw_checks_done_marker(self) -> Option<&'static str> {
        match self {
            Self::Done(m) => Some(m),
            Self::Ready(_) => None,
        }
    }
}

/// One field of one structured record, and what class of claim it makes.
#[derive(Clone, Copy, Debug)]
pub struct FieldSpec {
    pub record: &'static str,
    pub field: &'static str,
    pub class: FieldClass,
}

/// A repeated line the payload prints, parsed into an indexed series.
///
/// The compile harness's per-tick line is the motivating case: 92 lines, four
/// memory numbers and two timing numbers each, and the claim "all 184 memory
/// values identical" is only checkable if the series is parsed rather than
/// diffed as prose.
pub struct SeriesSpec {
    pub name: &'static str,
    pub description: &'static str,
    /// A regex with named captures. One capture is the index (`key`); the rest
    /// are values, each with a class.
    pattern: &'static str,
    pub key: &'static str,
    pub fields: &'static [(&'static str, FieldClass)],
    compiled: OnceLock<Regex>,
}

impl SeriesSpec {
    pub fn regex(&self) -> &Regex {
        self.compiled
            .get_or_init(|| Regex::new(self.pattern).expect("series pattern compiles"))
    }

    pub fn pattern(&self) -> &'static str {
        self.pattern
    }
}

impl std::fmt::Debug for SeriesSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SeriesSpec")
            .field("name", &self.name)
            .field("key", &self.key)
            .finish()
    }
}

/// A payload: one named, runnable question.
#[derive(Debug)]
pub struct Payload {
    pub name: &'static str,
    pub display_name: &'static str,
    /// The `fw-checks` `FwCheck::slug()` this mirrors.
    pub fw_check_slug: &'static str,
    /// The cargo feature on the firmware crate that builds it.
    pub firmware_feature: &'static str,
    /// The cargo feature on `fw-checks` that compiles its shared module.
    pub fw_checks_feature: &'static str,
    pub sentinel: Sentinel,
    /// Structured record kinds it emits behind `[fw-check-json] `.
    pub record_kinds: &'static [&'static str],
    /// The mask set that makes two of its transcripts comparable.
    pub mask_set: &'static str,
    pub fields: &'static [FieldSpec],
    pub series: &'static [&'static SeriesSpec],
}

impl Payload {
    pub fn class_of(&self, record: &str, field: &str) -> Option<FieldClass> {
        self.fields
            .iter()
            .find(|f| f.record == record && f.field == field)
            .map(|f| f.class)
    }
}

/// The compile harness's per-tick line.
///
/// ```text
/// [inc-shader-compile] case=examples-basic tick=1 stage= slice_cycles=173595 \
///   slice_us=1084 mem_before=321600 free/3936 used mem_after=308508 free/17028 used
/// ```
pub static COMPILE_TICK: SeriesSpec = SeriesSpec {
    name: "compile-tick",
    description: "one incremental-compile slice: its cost and the heap either side",
    pattern: concat!(
        r"case=(?<case>\S+) tick=(?<tick>\d+) stage=(?<stage>\S*) ",
        r"slice_cycles=(?<slice_cycles>\d+) slice_us=(?<slice_us>\d+) ",
        r"mem_before=(?<mem_before_free>\d+) free/(?<mem_before_used>\d+) used ",
        r"mem_after=(?<mem_after_free>\d+) free/(?<mem_after_used>\d+) used",
    ),
    key: "tick",
    fields: &[
        ("case", FieldClass::Structural),
        ("stage", FieldClass::Structural),
        ("slice_cycles", FieldClass::Timing),
        ("slice_us", FieldClass::Timing),
        ("mem_before_free", FieldClass::Memory),
        ("mem_before_used", FieldClass::Memory),
        ("mem_after_free", FieldClass::Memory),
        ("mem_after_used", FieldClass::Memory),
    ],
    compiled: OnceLock::new(),
};

/// The GPIO calibration protocol's pulse report.
///
/// ```text
/// CAL PULSE gpio=18 duty=40
/// ```
pub static CAL_PULSE: SeriesSpec = SeriesSpec {
    name: "cal-pulse",
    description: "one duty-ramp report from the host-driven GPIO calibration payload",
    pattern: r"^CAL PULSE gpio=(?<gpio>\d+) duty=(?<duty>\d+)$",
    key: "gpio",
    fields: &[("duty", FieldClass::Pin)],
    compiled: OnceLock::new(),
};

pub static ALL_PAYLOADS: &[Payload] = &[
    Payload {
        name: "shader-compile-stress",
        display_name: "Incremental shader compile stress",
        fw_check_slug: "shader-compile-stress",
        firmware_feature: "test_shader_compile_incremental",
        fw_checks_feature: "check-shader-compile",
        sentinel: Sentinel::Done("[inc-shader-compile] === DONE ==="),
        record_kinds: &["case-summary", "total-summary"],
        mask_set: "compile-harness",
        fields: &[
            FieldSpec {
                record: "case-summary",
                field: "case",
                class: FieldClass::Structural,
            },
            FieldSpec {
                record: "case-summary",
                field: "ticks",
                class: FieldClass::Structural,
            },
            FieldSpec {
                record: "case-summary",
                field: "max_slice_stage",
                class: FieldClass::Structural,
            },
            FieldSpec {
                record: "case-summary",
                field: "build_us",
                class: FieldClass::Timing,
            },
            FieldSpec {
                record: "case-summary",
                field: "max_slice_us",
                class: FieldClass::Timing,
            },
            FieldSpec {
                record: "case-summary",
                field: "peak_used",
                class: FieldClass::Memory,
            },
            FieldSpec {
                record: "case-summary",
                field: "resident_used",
                class: FieldClass::Memory,
            },
            FieldSpec {
                record: "case-summary",
                field: "after_drop_used",
                class: FieldClass::Memory,
            },
            FieldSpec {
                record: "total-summary",
                field: "cases",
                class: FieldClass::Structural,
            },
            FieldSpec {
                record: "total-summary",
                field: "build_us",
                class: FieldClass::Timing,
            },
            FieldSpec {
                record: "total-summary",
                field: "worst_slice_us",
                class: FieldClass::Timing,
            },
            FieldSpec {
                record: "total-summary",
                field: "worst_peak_used",
                class: FieldClass::Memory,
            },
        ],
        series: &[&COMPILE_TICK],
    },
    Payload {
        name: "gpio-calibrate",
        display_name: "Host-driven GPIO square-wave calibration",
        fw_check_slug: "gpio-calibrate",
        firmware_feature: "test_gpio_calibrate",
        fw_checks_feature: "check-gpio-calibrate",
        sentinel: Sentinel::Ready("CAL READY target="),
        record_kinds: &[],
        mask_set: "normalize",
        fields: &[],
        series: &[&CAL_PULSE],
    },
];

pub fn find_payload(name: &str) -> Result<&'static Payload> {
    match ALL_PAYLOADS.iter().find(|p| p.name == name) {
        Some(p) => Ok(p),
        None => bail!(
            "unknown payload `{name}` (known: {})",
            ALL_PAYLOADS
                .iter()
                .map(|p| p.name)
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_names_are_unique() {
        let mut names: Vec<_> = ALL_PAYLOADS.iter().map(|p| p.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate payload name");
    }

    #[test]
    fn every_payload_names_a_known_mask_set() {
        for p in ALL_PAYLOADS {
            crate::mask::mask_set(p.mask_set).unwrap_or_else(|e| panic!("payload {}: {e}", p.name));
        }
    }

    #[test]
    fn every_series_pattern_compiles_and_names_its_key() {
        for p in ALL_PAYLOADS {
            for s in p.series {
                let re = s.regex();
                let names: Vec<_> = re.capture_names().flatten().collect();
                assert!(
                    names.contains(&s.key),
                    "{}: key `{}` is not a capture in {:?}",
                    s.name,
                    s.key,
                    names
                );
                for (field, _) in s.fields {
                    assert!(
                        names.contains(field),
                        "{}: field `{field}` is not a capture",
                        s.name
                    );
                }
            }
        }
    }

    #[test]
    fn compile_tick_parses_a_real_line() {
        let line = "[INFO] fw_esp32c6::tests::incremental_shader_compile::runner: \
                    [inc-shader-compile] case=examples-basic tick=1 stage= \
                    slice_cycles=173595 slice_us=1084 mem_before=321600 free/3936 used \
                    mem_after=308508 free/17028 used";
        let caps = COMPILE_TICK.regex().captures(line).expect("matches");
        assert_eq!(&caps["tick"], "1");
        assert_eq!(&caps["case"], "examples-basic");
        assert_eq!(&caps["stage"], "");
        assert_eq!(&caps["slice_us"], "1084");
        assert_eq!(&caps["mem_before_used"], "3936");
        assert_eq!(&caps["mem_after_used"], "17028");
    }

    #[test]
    fn cal_pulse_parses_a_protocol_line() {
        let caps = CAL_PULSE
            .regex()
            .captures("CAL PULSE gpio=18 duty=40")
            .expect("matches");
        assert_eq!(&caps["gpio"], "18");
        assert_eq!(&caps["duty"], "40");
        assert!(
            CAL_PULSE
                .regex()
                .captures("CAL READY target=esp32c6")
                .is_none()
        );
    }

    #[test]
    fn lookup_reports_the_known_set() {
        assert!(find_payload("shader-compile-stress").is_ok());
        let err = find_payload("nope").unwrap_err().to_string();
        assert!(err.contains("shader-compile-stress"), "{err}");
    }
}
