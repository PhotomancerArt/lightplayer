//! `validate.toml`: the sets and the configuration table.
//!
//! Two things live here rather than in Rust, because they are *policy* and
//! change without a code change: which payloads make up a named set, and what
//! each configuration is trusted for. The payload registry itself stays in
//! Rust (`payload.rs`) — it carries regexes and field classes that a TOML file
//! could only hold as strings nobody checks.
//!
//! The file is compiled in with `include_str!`, so the runner works from any
//! directory; `ValidateConfig::load` reads an override path when a caller wants
//! one.

use std::borrow::Cow;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::configuration::{Configuration, TrustTable};
use crate::payload::{Payload, find_payload};

const EMBEDDED: &str = include_str!("../validate.toml");

#[derive(Clone, Debug, Deserialize)]
pub struct ValidateConfig {
    #[serde(default, rename = "set")]
    pub sets: Vec<PayloadSet>,
    #[serde(default, rename = "configuration")]
    pub configurations: Vec<ConfigurationEntry>,
    /// Emulator seam overlays (ADR docs/adr/2026-10-05-emulator-seams.md):
    /// what one seam implementation changes about a configuration's trust.
    /// `lp-emu:esp32c6:t2+led=fast` is the base configuration with each
    /// atom's overlay laid over it, in label order — nobody writes one table
    /// per combination.
    #[serde(default, rename = "seam")]
    pub seams: Vec<SeamOverlay>,
}

/// What a seam's kind means here: a performance seam never makes a
/// transcript; a capability seam may.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SeamKind {
    Performance,
    Capability,
}

/// One `[[seam]]` overlay: a seam implementation's own trust entries.
#[derive(Clone, Debug, Deserialize)]
pub struct SeamOverlay {
    /// The label's seam name (`led`).
    pub name: String,
    /// The implementation (`fast`).
    pub implementation: String,
    pub kind: SeamKind,
    pub description: String,
    /// Entries that replace the base's for their class (`grade = "absent"`
    /// marks a class this implementation does not produce at all).
    #[serde(default)]
    pub trust: TrustTable,
}

impl SeamOverlay {
    /// `led=fast`.
    pub fn atom(&self) -> String {
        format!("{}={}", self.name, self.implementation)
    }
}

/// The label's marker for a run's pace, after its seam atoms:
/// `lp-emu:esp32c6:t1+net=lan@pace=realtime`. The emulator writes it
/// (`lp_emu_esp_common::seam::net::Pace::label_suffix`); `lp-cli` owns the
/// test that the two spellings agree. Not a `+` atom: a pace is not a seam,
/// and no overlay ever answers for one.
pub const PACE_MARKER: &str = "@pace=";

/// The label's marker for a run with a flash power-cut plan armed, after
/// its seam atoms and before its pace: `lp-emu:esp32c6:t1+flash-cut`. The
/// emulator writes it (`lp_emu_esp32c6::flash_cut_spec::FLASH_CUT_MARKER`);
/// `lp-cli` owns the test that the two spellings agree. A `+` atom with no
/// `=`: it is a fault injection, not a seam, and no overlay answers for it.
pub const FLASH_CUT_MARKER: &str = "+flash-cut";

/// A run's pace, as a label names it (`@pace=<word>`). An unset pace names
/// nothing, and is every transcript's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelPace {
    /// 1×: held to wall time for the whole run. Wall-clock dependent, so it
    /// never makes a transcript.
    Realtime,
    /// As fast as possible, never paced.
    Max,
}

impl LabelPace {
    /// `realtime` / `max`.
    pub fn as_str(self) -> &'static str {
        match self {
            LabelPace::Realtime => "realtime",
            LabelPace::Max => "max",
        }
    }

    fn parse(word: &str) -> Option<Self> {
        [LabelPace::Realtime, LabelPace::Max]
            .into_iter()
            .find(|p| p.as_str() == word)
    }
}

/// One engaged seam in a composed configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeamAtom {
    pub seam: String,
    pub implementation: String,
    pub kind: SeamKind,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PayloadSet {
    pub name: String,
    pub description: String,
    pub payloads: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ConfigurationEntry {
    pub name: String,
    pub description: String,
    pub chip: String,
    /// The chip identity this configuration reports, when it has to be told.
    ///
    /// Silicon reads its own eFuse and leaves these empty — a transcript from
    /// a board records what that board said. An emulator has no eFuse to read,
    /// so ours is given the desk board's MAC and revision here, and the runner
    /// passes them on the machine's command line. That is what makes the
    /// identity fields of a hello frame compare equal across a silicon
    /// transcript and an emulated one instead of differing for a reason that
    /// says nothing about the model.
    ///
    /// Still **not** identity in the PD4 sense: the configuration is the chip
    /// (`lp-emu:esp32c6:t1`), and which board these numbers came from is said
    /// in `board`, here and in every transcript's sidecar.
    #[serde(default)]
    pub mac: Option<String>,
    #[serde(default)]
    pub silicon_rev: Option<String>,
    #[serde(default)]
    pub board: Option<String>,
    /// Can this configuration produce a decoded pin capture?
    ///
    /// **Stated, never inferred**, for the same reason trust is: silence is
    /// not a capability. An `lp-emu:*` machine decodes the pad off its own
    /// signal fabric and says `records_pins = true`; silicon says nothing and
    /// therefore records none, because reading a real pad needs an instrument
    /// nobody has put on this bench.
    ///
    /// Deliberately not derived from the `lp-emu:` name prefix. That would be
    /// right by accident and wrong the first time a configuration is a board
    /// with a logic analyser on it — which is exactly the capture the `pin`
    /// class is waiting for
    /// (`docs/defects/2026-09-08-a-pin-capture-is-a-property-of-the-configuration-not-the-payload.md`).
    #[serde(default)]
    pub records_pins: bool,
    #[serde(default)]
    pub trust: TrustTable,
    /// The seams composed onto this entry (`…+led=fast`), in label order.
    /// Empty for every entry `validate.toml` names directly; `name` stays the
    /// base configuration's, and [`label`](Self::label) is the composite.
    #[serde(skip)]
    pub seams: Vec<SeamAtom>,
    /// The run's pace when the name set one (`…@pace=max`); `None` for every
    /// entry `validate.toml` names directly, and for every transcript.
    #[serde(skip)]
    pub pace: Option<LabelPace>,
    /// The name carried `+flash-cut`: a run whose flash a power cut tore.
    /// `false` for every entry `validate.toml` names directly, and for every
    /// transcript.
    #[serde(skip)]
    pub flash_cut: bool,
}

impl ConfigurationEntry {
    pub fn parsed(&self) -> Result<Configuration> {
        Configuration::parse(&self.name)
    }

    /// The configuration label: the base name plus one `+<seam>=<impl>` per
    /// composed seam, then `+flash-cut` when the name carried it, then
    /// `@pace=<mode>` when a pace was set. Exactly `name` with none.
    pub fn label(&self) -> String {
        let mut out = self.seam_label();
        if let Some(pace) = self.pace {
            out.push_str(PACE_MARKER);
            out.push_str(pace.as_str());
        }
        out
    }

    /// The label without its pace: the base name, its seam atoms and its
    /// flash-cut marker.
    pub fn seam_label(&self) -> String {
        let mut out = self.name.clone();
        for s in &self.seams {
            out.push('+');
            out.push_str(&s.seam);
            out.push('=');
            out.push_str(&s.implementation);
        }
        if self.flash_cut {
            out.push_str(FLASH_CUT_MARKER);
        }
        out
    }

    /// The first composed performance seam, if any: such a configuration
    /// never makes a transcript.
    pub fn performance_seam(&self) -> Option<&SeamAtom> {
        self.seams.iter().find(|s| s.kind == SeamKind::Performance)
    }

    /// The identity to hand a driver.
    pub fn identity(&self) -> crate::driver::Identity {
        crate::driver::Identity {
            mac: self.mac.clone(),
            silicon_rev: self.silicon_rev.clone(),
            board: self.board.clone(),
        }
    }
}

impl ValidateConfig {
    /// The table compiled into this binary.
    pub fn embedded() -> Self {
        Self::parse(EMBEDDED).expect("the embedded validate.toml parses and validates")
    }

    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let path = path.as_ref();
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("in {}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let cfg: Self = toml::from_str(text).context("parsing validate.toml")?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<()> {
        for set in &self.sets {
            if set.payloads.is_empty() {
                bail!("set `{}` lists no payloads", set.name);
            }
            for name in &set.payloads {
                find_payload(name).with_context(|| format!("in set `{}`", set.name))?;
            }
        }
        for c in &self.configurations {
            c.parsed()
                .with_context(|| format!("in configuration `{}`", c.name))?;
        }
        let mut atoms: Vec<String> = self.seams.iter().map(SeamOverlay::atom).collect();
        atoms.sort_unstable();
        let before = atoms.len();
        atoms.dedup();
        if atoms.len() != before {
            bail!("duplicate [[seam]] overlay in validate.toml");
        }
        for o in &self.seams {
            if o.description.trim().is_empty() {
                bail!("[[seam]] `{}` says nothing about what it changes", o.atom());
            }
        }
        let mut names: Vec<&str> = self.sets.iter().map(|s| s.name.as_str()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        if names.len() != before {
            bail!("duplicate set name in validate.toml");
        }
        let mut names: Vec<&str> = self
            .configurations
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        if names.len() != before {
            bail!("duplicate configuration name in validate.toml");
        }
        Ok(())
    }

    pub fn set(&self, name: &str) -> Result<&PayloadSet> {
        match self.sets.iter().find(|s| s.name == name) {
            Some(s) => Ok(s),
            None => bail!(
                "unknown set `{name}` (known: {})",
                self.sets
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }

    /// The payloads a set names — or the one payload, when `set` is a
    /// payload's own name and no set has that name.
    ///
    /// The fallback is not a convenience. M6 records `emu-m6`'s four
    /// transcripts against **two** images (sitting 1's commit for the
    /// DD30 arbitration, main's for the three scenarios whose vehicle P1b
    /// added afterwards), and `record` takes one `--commit` per invocation
    /// because the commit is provenance, not a guess. Without this a set of
    /// four would have to be split into sets of one in `validate.toml`,
    /// which would make the policy file a workaround for the CLI.
    pub fn payloads_in(&self, set: &str) -> Result<Vec<&'static Payload>> {
        if self.sets.iter().all(|s| s.name != set)
            && let Ok(one) = find_payload(set)
        {
            return Ok(vec![one]);
        }
        self.set(set)?
            .payloads
            .iter()
            .map(|n| find_payload(n))
            .collect()
    }

    /// The configuration `name` names: an entry of the table, or a composite
    /// `<base>+<seam>=<impl>…[+flash-cut][@pace=<mode>]` — the base entry
    /// with each atom's `[[seam]]` overlay laid over its trust, in label
    /// order, the flash-cut mark, and the run's pace when one was set
    /// (neither moves a grade). A name with no `+` and no `@` is exactly the
    /// table's entry, as it always was.
    pub fn configuration(&self, name: &str) -> Result<Cow<'_, ConfigurationEntry>> {
        // The pace comes off first, so the seam atoms never see it.
        let (seamed, pace) = match name.split_once('@') {
            None => (name, None),
            Some((seamed, modifier)) => {
                let pace = modifier
                    .strip_prefix(&PACE_MARKER[1..])
                    .and_then(LabelPace::parse)
                    .with_context(|| {
                        format!(
                            "`{name}`: `@{modifier}` is not a pace — `{PACE_MARKER}realtime` or \
                             `{PACE_MARKER}max`, after the seam atoms"
                        )
                    })?;
                (seamed, Some(pace))
            }
        };
        let mut parts = seamed.split('+');
        let base_name = parts.next().unwrap_or_default();
        let base = match self.configurations.iter().find(|c| c.name == base_name) {
            Some(c) => c,
            None => bail!(
                "unknown configuration `{base_name}` (known: {})",
                self.configurations
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        };
        // Label order is sorted (the ADR's label rule), whatever order the
        // caller typed: `…+net=lan+led=fast` and `…+led=fast+net=lan` are one
        // configuration, composed the same way and labelled the same.
        let mut atoms: Vec<&str> = parts.collect();
        atoms.sort_unstable();
        // `+flash-cut` is not a seam: it marks the run, and no overlay
        // answers for it (`refuse_a_flash_cut` is what reads it).
        let flash_cut = atoms.iter().any(|a| *a == &FLASH_CUT_MARKER[1..]);
        atoms.retain(|a| *a != &FLASH_CUT_MARKER[1..]);
        if atoms.is_empty() && pace.is_none() && !flash_cut {
            return Ok(Cow::Borrowed(base));
        }
        let mut composed = base.clone();
        composed.pace = pace;
        composed.flash_cut = flash_cut;
        for atom in atoms {
            let overlay = self
                .seams
                .iter()
                .find(|o| o.atom() == atom)
                .with_context(|| {
                    format!(
                        "`{name}`: no [[seam]] overlay `{atom}` (known: {})",
                        self.seams
                            .iter()
                            .map(SeamOverlay::atom)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })?;
            if composed.seams.iter().any(|s| s.seam == overlay.name) {
                bail!("`{name}`: seam `{}` named twice", overlay.name);
            }
            composed.trust = composed.trust.overlaid(&overlay.trust);
            composed.seams.push(SeamAtom {
                seam: overlay.name.clone(),
                implementation: overlay.implementation.clone(),
                kind: overlay.kind,
            });
        }
        Ok(Cow::Owned(composed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grade::{FieldClass, Grade};

    #[test]
    fn the_embedded_table_parses() {
        let cfg = ValidateConfig::embedded();
        assert!(!cfg.sets.is_empty());
        assert!(!cfg.configurations.is_empty());
    }

    #[test]
    fn every_set_names_known_payloads() {
        let cfg = ValidateConfig::embedded();
        for set in &cfg.sets {
            cfg.payloads_in(&set.name).unwrap();
        }
    }

    #[test]
    fn esp_emu_is_trusted_for_memory_and_not_for_time() {
        let cfg = ValidateConfig::embedded();
        let e = cfg.configuration("esp-emu:0.42.0").unwrap();
        assert_eq!(e.trust.grade(FieldClass::Memory), Grade::Measured);
        assert_eq!(e.trust.grade(FieldClass::Timing), Grade::Modeled);
        assert_eq!(e.trust.grade(FieldClass::UsbSerialJtag), Grade::Modeled);
        assert!(e.trust.because(FieldClass::Memory).is_some());
    }

    #[test]
    fn silicon_is_measured_everywhere_it_claims_anything() {
        let cfg = ValidateConfig::embedded();
        let s = cfg.configuration("silicon:esp32c6").unwrap();
        for class in [
            FieldClass::Memory,
            FieldClass::Timing,
            FieldClass::Pin,
            FieldClass::UsbSerialJtag,
            FieldClass::BootLog,
            FieldClass::Wire,
        ] {
            assert_eq!(
                s.trust.grade(class),
                Grade::Measured,
                "silicon should be measured for {class}"
            );
        }
    }

    /// 2026-10-03: frame rate under link load is not a grade this table
    /// backs on any emulated chip — the S3 and classic desk sittings (PRs
    /// #942/#943) found it off by 4-14x with the opposite sign from the C6's
    /// own cold-code-path defect, and nothing promotes `timing` to fix it.
    /// This is a cheap tripwire: if a future edit to `validate.toml` drops
    /// the caveat from one of these `because` strings, this fails instead of
    /// silently letting an agent quote an emulated fps-under-load figure
    /// again. It does not grade anything and does not change a grade.
    #[test]
    fn frame_rate_under_link_load_is_not_graded_on_any_emulated_chip() {
        let cfg = ValidateConfig::embedded();
        for name in [
            "lp-emu:esp32c6:t1",
            "lp-emu:esp32c6:t2",
            "lp-emu:esp32c6:t3",
            "lp-emu:esp32v3:t1",
            "lp-emu:esp32s3:t1",
        ] {
            let entry = cfg.configuration(name).unwrap();
            // The grade stays whatever it already was (`modeled` or, for
            // t3, `documented`) — this test asserts the caveat text exists,
            // never a grade.
            let why = entry
                .trust
                .because(FieldClass::Timing)
                .unwrap_or_else(|| panic!("{name}: no `timing` trust entry"));
            assert!(
                why.to_lowercase().contains("frame rate under link load")
                    && why.to_lowercase().contains("not graded"),
                "{name}: `timing` trust entry does not say frame rate under \
                 link load is not graded: `{why}`"
            );
        }
    }

    #[test]
    fn a_set_naming_an_unknown_payload_is_refused() {
        let err = ValidateConfig::parse(
            r#"
[[set]]
name = "bad"
description = "x"
payloads = ["no-such-payload"]
"#,
        )
        .unwrap_err();
        assert!(format!("{err:#}").contains("no-such-payload"));
    }

    #[test]
    fn duplicate_set_names_are_refused() {
        let err = ValidateConfig::parse(
            r#"
[[set]]
name = "a"
description = "x"
payloads = ["gpio-calibrate"]

[[set]]
name = "a"
description = "y"
payloads = ["gpio-calibrate"]
"#,
        )
        .unwrap_err();
        assert!(format!("{err:#}").contains("duplicate set name"));
    }

    #[test]
    fn an_unparseable_configuration_name_is_refused() {
        let err = ValidateConfig::parse(
            r#"
[[configuration]]
name = "qemu:esp32c6"
description = "x"
chip = "esp32c6"
"#,
        )
        .unwrap_err();
        assert!(format!("{err:#}").contains("qemu"));
    }

    #[test]
    fn a_composite_name_is_the_base_with_its_overlay_and_a_plain_name_is_unchanged() {
        let cfg = ValidateConfig::embedded();
        let base = cfg.configuration("lp-emu:esp32c6:t2").unwrap();
        assert!(
            matches!(base, Cow::Borrowed(_)),
            "a plain name is the entry itself"
        );
        assert!(base.seams.is_empty());
        assert_eq!(base.label(), "lp-emu:esp32c6:t2");

        let led = cfg.configuration("lp-emu:esp32c6:t2+led=fast").unwrap();
        assert_eq!(led.name, "lp-emu:esp32c6:t2", "the base keeps its name");
        assert_eq!(led.label(), "lp-emu:esp32c6:t2+led=fast");
        assert_eq!(led.seams.len(), 1);
        assert_eq!(led.performance_seam().unwrap().seam, "led");
        // The overlay replaces timing's reason and leaves every other class,
        // pin included (L1 keeps the pads), as the base had it.
        assert!(
            led.trust
                .because(FieldClass::Timing)
                .unwrap()
                .contains("refill-latency"),
            "{:?}",
            led.trust.because(FieldClass::Timing)
        );
        for class in FieldClass::ALL.iter().filter(|c| **c != FieldClass::Timing) {
            assert_eq!(led.trust.grade(*class), base.trust.grade(*class), "{class}");
            assert_eq!(
                led.trust.because(*class),
                base.trust.because(*class),
                "{class}"
            );
        }
        assert_eq!(led.parsed().unwrap().name(), "lp-emu:esp32c6:t2");
    }

    /// `net=lan` composes onto every C6 grade as a capability seam, moves no
    /// grade anywhere (the layers above the frame device are the base's), and
    /// keeps t3's documented timing band — the label of nearly every emulated
    /// C6 run must grade exactly what the seam-free run grades.
    #[test]
    fn net_lan_is_a_capability_overlay_that_moves_no_grade() {
        let cfg = ValidateConfig::embedded();
        for base_name in [
            "lp-emu:esp32c6:t1",
            "lp-emu:esp32c6:t2",
            "lp-emu:esp32c6:t3",
        ] {
            let base = cfg.configuration(base_name).unwrap();
            let label = format!("{base_name}+net=lan");
            let net = cfg.configuration(&label).unwrap();
            assert_eq!(net.label(), label);
            assert_eq!(net.name, base_name, "the base keeps its name");
            assert!(net.performance_seam().is_none(), "a capability seam");
            assert_eq!(net.seams[0].kind, SeamKind::Capability);
            assert_eq!(net.records_pins, base.records_pins);
            for class in FieldClass::ALL {
                assert_eq!(
                    net.trust.grade(*class),
                    base.trust.grade(*class),
                    "{label}: {class}"
                );
                if *class != FieldClass::Memory {
                    assert_eq!(
                        net.trust.because(*class),
                        base.trust.because(*class),
                        "{label}: {class}'s reason is the base's"
                    );
                }
            }
            assert_eq!(
                net.trust.band(FieldClass::Timing).map(|b| b.describe()),
                base.trust.band(FieldClass::Timing).map(|b| b.describe()),
                "{label}: the timing band survives"
            );
            assert!(
                net.trust
                    .because(FieldClass::Memory)
                    .unwrap()
                    .contains("join allocations never happen"),
                "{label}: memory carries the driver caveat"
            );
        }
        // Label order is sorted whatever order the caller typed.
        let a = cfg
            .configuration("lp-emu:esp32c6:t2+net=lan+led=fast")
            .unwrap();
        let b = cfg
            .configuration("lp-emu:esp32c6:t2+led=fast+net=lan")
            .unwrap();
        assert_eq!(a.label(), "lp-emu:esp32c6:t2+led=fast+net=lan");
        assert_eq!(a.label(), b.label());
        assert_eq!(a.performance_seam().unwrap().seam, "led");
    }

    #[test]
    fn an_unknown_or_doubled_atom_is_refused_by_name() {
        let cfg = ValidateConfig::embedded();
        let err = format!(
            "{:#}",
            cfg.configuration("lp-emu:esp32c6:t2+led=slow").unwrap_err()
        );
        assert!(err.contains("no [[seam]] overlay `led=slow`"), "{err}");
        assert!(
            err.contains("led=fast"),
            "the refusal lists the known ones: {err}"
        );
        let err = format!(
            "{:#}",
            cfg.configuration("lp-emu:esp32c6:t2+led=fast+led=fast")
                .unwrap_err()
        );
        assert!(err.contains("named twice"), "{err}");
        assert!(cfg.configuration("nope+led=fast").is_err());
    }

    /// A pace rides after the seam atoms, is never read as one, moves no
    /// grade, and an unset pace leaves every label as it was.
    #[test]
    fn a_pace_follows_the_atoms_and_is_never_a_seam() {
        let cfg = ValidateConfig::embedded();
        let plain = cfg.configuration("lp-emu:esp32c6:t1+net=lan").unwrap();
        assert_eq!(plain.pace, None);
        for (word, pace) in [("realtime", LabelPace::Realtime), ("max", LabelPace::Max)] {
            let label = format!("lp-emu:esp32c6:t1+net=lan@pace={word}");
            let paced = cfg.configuration(&label).unwrap();
            assert_eq!(paced.pace, Some(pace));
            assert_eq!(paced.label(), label);
            assert_eq!(paced.seam_label(), "lp-emu:esp32c6:t1+net=lan");
            assert_eq!(paced.seams, plain.seams, "no seam atom for the pace");
            for class in FieldClass::ALL {
                assert_eq!(paced.trust.grade(*class), plain.trust.grade(*class));
            }
            // With no seam engaged, beside the bare base.
            let bare = cfg
                .configuration(&format!("lp-emu:esp32c6:t1@pace={word}"))
                .unwrap();
            assert_eq!(bare.label(), format!("lp-emu:esp32c6:t1@pace={word}"));
            assert!(bare.seams.is_empty());
        }
        for bad in [
            "lp-emu:esp32c6:t1@pace=fast",
            "lp-emu:esp32c6:t1@speed=max",
            "lp-emu:esp32c6:t1@pace=max+net=lan",
        ] {
            let err = format!("{:#}", cfg.configuration(bad).unwrap_err());
            assert!(err.contains("is not a pace"), "{bad}: {err}");
        }
        // A pace is not a `+` atom: spelled as one, it is an unknown seam.
        let err = format!(
            "{:#}",
            cfg.configuration("lp-emu:esp32c6:t1+pace=max").unwrap_err()
        );
        assert!(err.contains("no [[seam]] overlay `pace=max`"), "{err}");
    }

    #[test]
    fn an_overlay_may_mark_a_class_absent_and_strict_reads_it_as_below_measured() {
        let cfg = ValidateConfig::parse(
            r#"
[[configuration]]
name = "lp-emu:esp32c6:t1"
description = "x"
chip = "esp32c6"

[[configuration.trust]]
class = "usb-serial-jtag"
grade = "measured"
because = "a base grade the overlay below takes away"

[[seam]]
name = "usb"
implementation = "fast"
kind = "performance"
description = "a test overlay that answers the link in place of the model"

[[seam.trust]]
class = "usb-serial-jtag"
grade = "absent"
because = "the seam answers the link, so the model never produces this class"
"#,
        )
        .unwrap();
        let composed = cfg.configuration("lp-emu:esp32c6:t1+usb=fast").unwrap();
        let grade = composed.trust.grade(FieldClass::UsbSerialJtag);
        assert_eq!(grade, Grade::Absent);
        assert!(grade < Grade::Measured, "--strict refuses it");
        assert_eq!(
            cfg.configuration("lp-emu:esp32c6:t1")
                .unwrap()
                .trust
                .grade(FieldClass::UsbSerialJtag),
            Grade::Measured,
            "the base is untouched"
        );
    }
}
