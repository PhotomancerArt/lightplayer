//! Configurations: the named reference implementations a payload can run on.
//!
//! Plan PD4. A configuration is a value, not a mood — it goes in the transcript
//! header, in the runner's config table, and in the transcript's filename, so
//! "which thing produced this number" is never a matter of remembering.
//!
//! ```text
//! silicon:seeed/xiao-esp32-c6     a board on the desk
//! esp-emu:0.42.0                  Espressif's binary emulator, that version
//! lp-emu:esp32c6:t1               our machine, time grade 1
//! ```

use std::fmt;
use std::str::FromStr;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::grade::{FieldClass, Grade};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConfigurationKind {
    /// Real hardware on a real port.
    Silicon,
    /// Espressif's `esp-emu`, a binary-only third-party emulator.
    EspEmu,
    /// The machine this plan builds.
    LpEmu,
}

impl ConfigurationKind {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Silicon => "silicon",
            Self::EspEmu => "esp-emu",
            Self::LpEmu => "lp-emu",
        }
    }
}

impl fmt::Display for ConfigurationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

/// A parsed configuration name.
///
/// `detail` is the board id for silicon, the version for esp-emu, and the chip
/// for our own machine; `qualifier` carries our machine's time grade (`t1`),
/// which nothing else uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Configuration {
    pub kind: ConfigurationKind,
    pub detail: String,
    pub qualifier: Option<String>,
}

impl Configuration {
    pub fn parse(s: &str) -> Result<Self> {
        let mut parts = s.split(':');
        let kind = match parts.next() {
            Some("silicon") => ConfigurationKind::Silicon,
            Some("esp-emu") => ConfigurationKind::EspEmu,
            Some("lp-emu") => ConfigurationKind::LpEmu,
            Some(other) => bail!(
                "unknown configuration kind `{other}` in `{s}` \
                 (expected silicon:<board>, esp-emu:<version>, or lp-emu:<chip>[:<time-grade>])"
            ),
            None => bail!("empty configuration name"),
        };
        let detail = match parts.next() {
            Some(d) if !d.is_empty() => d.to_string(),
            _ => bail!(
                "configuration `{s}` needs a detail: \
                 silicon:<board>, esp-emu:<version>, lp-emu:<chip>[:<time-grade>]"
            ),
        };
        let qualifier = parts.next().map(str::to_string);
        if parts.next().is_some() {
            bail!("configuration `{s}` has more than three `:`-separated parts");
        }
        if qualifier.is_some() && kind != ConfigurationKind::LpEmu {
            bail!("only `lp-emu:<chip>:<time-grade>` takes a third part; got `{s}`");
        }
        Ok(Self {
            kind,
            detail,
            qualifier,
        })
    }

    /// The canonical name, as it appears in a header.
    pub fn name(&self) -> String {
        match &self.qualifier {
            Some(q) => format!("{}:{}:{q}", self.kind, self.detail),
            None => format!("{}:{}", self.kind, self.detail),
        }
    }

    /// The filename-safe form used in `<configuration>-<date>-<short>.txt`.
    ///
    /// `:` and `/` both become `-`, so `silicon:seeed/xiao-esp32-c6` files as
    /// `silicon-seeed-xiao-esp32-c6`.
    pub fn slug(&self) -> String {
        self.name().replace([':', '/'], "-")
    }

    /// Is this configuration a thing that can be run right now?
    ///
    /// `lp-emu:*` is the seam M3 fills; naming it here rather than omitting it
    /// is deliberate — `validate list` should say the configuration exists and
    /// is not ready, not pretend it was never planned.
    pub fn availability(&self) -> Availability {
        match self.kind {
            ConfigurationKind::LpEmu => Availability::UnavailableUntil("M3"),
            _ => Availability::Available,
        }
    }
}

impl FromStr for Configuration {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

impl fmt::Display for Configuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    Available,
    /// The milestone that will make it available.
    UnavailableUntil(&'static str),
}

impl fmt::Display for Availability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Available => f.write_str("available"),
            Self::UnavailableUntil(m) => write!(f, "unavailable until {m}"),
        }
    }
}

/// What a configuration is trusted for, per field class.
///
/// Loaded from `validate.toml`, never inferred. A configuration with no entry
/// for a class is `Modeled` there: silence is not trust.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TrustTable {
    entries: Vec<TrustEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrustEntry {
    pub class: FieldClass,
    pub grade: Grade,
    /// Why. A trust entry without a reason is a guess with a table around it.
    pub because: String,
}

impl TrustTable {
    pub fn new(entries: Vec<TrustEntry>) -> Self {
        Self { entries }
    }

    pub fn entries(&self) -> &[TrustEntry] {
        &self.entries
    }

    /// The grade this configuration earns for a field class.
    pub fn grade(&self, class: FieldClass) -> Grade {
        self.entries
            .iter()
            .find(|e| e.class == class)
            .map_or(Grade::Modeled, |e| e.grade)
    }

    pub fn because(&self, class: FieldClass) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.class == class)
            .map(|e| e.because.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_three_kinds() {
        let s = Configuration::parse("silicon:seeed/xiao-esp32-c6").unwrap();
        assert_eq!(s.kind, ConfigurationKind::Silicon);
        assert_eq!(s.detail, "seeed/xiao-esp32-c6");
        assert_eq!(s.qualifier, None);

        let e = Configuration::parse("esp-emu:0.42.0").unwrap();
        assert_eq!(e.kind, ConfigurationKind::EspEmu);
        assert_eq!(e.detail, "0.42.0");

        let l = Configuration::parse("lp-emu:esp32c6:t1").unwrap();
        assert_eq!(l.kind, ConfigurationKind::LpEmu);
        assert_eq!(l.detail, "esp32c6");
        assert_eq!(l.qualifier.as_deref(), Some("t1"));
    }

    #[test]
    fn names_round_trip() {
        for name in [
            "silicon:seeed/xiao-esp32-c6",
            "esp-emu:0.42.0",
            "lp-emu:esp32c6:t1",
            "lp-emu:esp32c6",
        ] {
            assert_eq!(Configuration::parse(name).unwrap().name(), name);
        }
    }

    #[test]
    fn slugs_are_filename_safe() {
        assert_eq!(
            Configuration::parse("silicon:seeed/xiao-esp32-c6")
                .unwrap()
                .slug(),
            "silicon-seeed-xiao-esp32-c6"
        );
        assert_eq!(
            Configuration::parse("lp-emu:esp32c6:t1").unwrap().slug(),
            "lp-emu-esp32c6-t1"
        );
    }

    #[test]
    fn rejects_nonsense() {
        for bad in [
            "silicon",
            "qemu:esp32c6",
            "silicon:board:extra",
            "esp-emu:",
            "lp-emu:esp32c6:t1:more",
        ] {
            assert!(
                Configuration::parse(bad).is_err(),
                "`{bad}` should not parse"
            );
        }
    }

    #[test]
    fn lp_emu_is_unavailable_until_m3() {
        assert_eq!(
            Configuration::parse("lp-emu:esp32c6:t1")
                .unwrap()
                .availability(),
            Availability::UnavailableUntil("M3")
        );
        assert_eq!(
            Configuration::parse("esp-emu:0.42.0")
                .unwrap()
                .availability(),
            Availability::Available
        );
    }

    #[test]
    fn silence_is_not_trust() {
        let t = TrustTable::new(vec![TrustEntry {
            class: FieldClass::Memory,
            grade: Grade::Measured,
            because: "desk-validated".into(),
        }]);
        assert_eq!(t.grade(FieldClass::Memory), Grade::Measured);
        assert_eq!(t.grade(FieldClass::Timing), Grade::Modeled);
        assert_eq!(t.grade(FieldClass::UsbSerialJtag), Grade::Modeled);
    }
}
