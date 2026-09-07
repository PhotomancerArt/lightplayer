//! Provenance grading: how a claim came to be true.
//!
//! Every field a transcript carries is graded by the *configuration that
//! produced it*, not by how confident the reader feels. The cautionary tale is
//! esp-emu's USB-Serial-JTAG model, which asserts SOF forever and reports EP1
//! free forever: the firmware serves into the void believing a host is
//! attached, and nothing in the transcript says "this number is invented".
//! Strict mode is what makes that impossible to repeat — it refuses a claim
//! whose grade is below `Measured`.

use std::fmt;

use serde::{Deserialize, Serialize};

/// How a value in a transcript was established.
///
/// Ordered worst-to-best so `>=` reads naturally: `grade >= Grade::Measured`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Grade {
    /// A model produced it. Nobody has checked it against the thing it models.
    Modeled,
    /// A datasheet, TRM, or vendor document says so, and the implementation
    /// follows the document — but no measurement on this path exists.
    Documented,
    /// Measured on silicon, or measured on a configuration whose agreement
    /// with silicon *for this class of field* is itself in a committed
    /// transcript.
    Measured,
}

impl Grade {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Modeled => "modeled",
            Self::Documented => "documented",
            Self::Measured => "measured",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "modeled" => Some(Self::Modeled),
            "documented" => Some(Self::Documented),
            "measured" => Some(Self::Measured),
            _ => None,
        }
    }
}

impl fmt::Display for Grade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

/// What kind of thing a transcript field measures.
///
/// A configuration is trusted per class, never wholesale: the spike proved
/// esp-emu byte-equal on memory and 2.4x wrong on time in the same run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FieldClass {
    /// Heap and stack figures: bytes free, used, peak, high-water.
    Memory,
    /// Anything with a clock in it: microseconds, cycles, fps, uptime.
    Timing,
    /// Observed pin state or a decoded waveform.
    Pin,
    /// USB-Serial-JTAG endpoint and host-attachment state.
    UsbSerialJtag,
    /// ROM and bootloader log output.
    BootLog,
    /// Wire-protocol bytes and the messages decoded from them.
    Wire,
    /// Counts and identities that carry no unit: tick numbers, case names,
    /// record kinds. Always compared, never masked.
    Structural,
}

impl FieldClass {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Timing => "timing",
            Self::Pin => "pin",
            Self::UsbSerialJtag => "usb-serial-jtag",
            Self::BootLog => "boot-log",
            Self::Wire => "wire",
            Self::Structural => "structural",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "memory" => Some(Self::Memory),
            "timing" => Some(Self::Timing),
            "pin" => Some(Self::Pin),
            "usb-serial-jtag" => Some(Self::UsbSerialJtag),
            "boot-log" => Some(Self::BootLog),
            "wire" => Some(Self::Wire),
            "structural" => Some(Self::Structural),
            _ => None,
        }
    }

    pub const ALL: &'static [FieldClass] = &[
        FieldClass::Memory,
        FieldClass::Timing,
        FieldClass::Pin,
        FieldClass::UsbSerialJtag,
        FieldClass::BootLog,
        FieldClass::Wire,
        FieldClass::Structural,
    ];
}

impl fmt::Display for FieldClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grades_order_worst_to_best() {
        assert!(Grade::Measured > Grade::Documented);
        assert!(Grade::Documented > Grade::Modeled);
    }

    #[test]
    fn grade_slugs_round_trip() {
        for g in [Grade::Modeled, Grade::Documented, Grade::Measured] {
            assert_eq!(Grade::parse(g.slug()), Some(g));
        }
    }

    #[test]
    fn field_class_slugs_round_trip() {
        for c in FieldClass::ALL {
            assert_eq!(FieldClass::parse(c.slug()), Some(*c));
        }
    }
}
