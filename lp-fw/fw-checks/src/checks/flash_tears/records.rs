//! The payload's `[fw-check-json] ` records, as hand-written JSON.
//!
//! Hand-written for the reason `header.rs` gives: no `alloc`, no serde, and
//! a record is one line. Every record has a `kind`; the host parsers
//! (`lp-emu-validate`'s registry and `scripts/emu/flash-tears-analyze.py`)
//! read these exact field names.

use core::fmt;

use super::analysis::{InFlight, ProgramTear, Settled};
use super::journal::CopyScan;

/// `ft-boot`: one per boot, first.
pub struct BootRecord<'a> {
    /// The chip's reset reason, as the firmware names it.
    pub reset: &'a str,
    /// The chip's base MAC, from eFuse: which board this boot is.
    pub mac: [u8; 6],
    /// The flash part's JEDEC id (manufacturer, type, capacity), as the
    /// part answers RDID: the tear behaviour belongs to this part.
    pub flash_id: u32,
    /// The payload's first sector (the start of `lpfs`).
    pub base: u32,
    /// The in-flight cycle the journal names, or `None` on a fresh region.
    pub latest: Option<u32>,
}

impl fmt::Display for BootRecord<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{\"kind\":\"ft-boot\",\"reset\":\"{}\",\"mac\":\"{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}\",\"flash_id\":\"0x{:06x}\",\"base\":\"0x{:x}\",\"region_sectors\":{},\"latest\":{},\"state\":\"{}\"}}",
            self.reset,
            self.mac[0],
            self.mac[1],
            self.mac[2],
            self.mac[3],
            self.mac[4],
            self.mac[5],
            self.flash_id,
            self.base,
            super::REGION_SECTORS,
            Opt(self.latest),
            if self.latest.is_some() {
                "resume"
            } else {
                "fresh"
            },
        )
    }
}

/// `ft-journal`: one per journal copy.
pub struct JournalRecord {
    pub copy: u32,
    pub scan: CopyScan,
}

impl fmt::Display for JournalRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{\"kind\":\"ft-journal\",\"copy\":{},\"valid\":{},\"damaged\":{},\"latest\":{},\"torn_next\":",
            self.copy,
            self.scan.valid,
            self.scan.damaged,
            Opt(self.scan.latest),
        )?;
        match &self.scan.torn_next {
            Some(t) => write!(f, "{}", TearJson(t))?,
            None => f.write_str("null")?,
        }
        f.write_str("}")
    }
}

/// `ft-sector` for a sector that was not in flight.
pub struct SettledRecord {
    pub latest: u32,
    pub sector: u32,
    /// The cycle that last wrote it.
    pub wrote: u32,
    pub settled: Settled,
}

impl fmt::Display for SettledRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{\"kind\":\"ft-sector\",\"latest\":{},\"sector\":{},\"role\":\"settled\",\"wrote\":{},\"verdict\":\"{}\",\"diff_bits\":{},\"weak_bits\":{}}}",
            self.latest,
            self.sector,
            self.wrote,
            if self.settled.complete {
                "complete"
            } else {
                "unexpected"
            },
            self.settled.diff_bits,
            self.settled.weak_bits,
        )
    }
}

/// `ft-sector` for the sector the in-flight cycle was writing.
pub struct InFlightRecord {
    pub latest: u32,
    pub sector: u32,
    pub f: InFlight,
}

impl fmt::Display for InFlightRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = &self.f;
        write!(
            f,
            "{{\"kind\":\"ft-sector\",\"latest\":{},\"sector\":{},\"role\":\"in-flight\",\"wrote\":{},\"verdict\":\"{}\",\
             \"weak_bits\":{},\"weak_bytes\":{},\"old_zero_bits\":{},\"remaining_old_zeros\":{},\"weak_at_old_zeros\":{},\
             \"new_zero_bits\":{},\"landed_new_zeros\":{},\"weak_at_new_zeros\":{},\"ff_bytes\":{},\
             \"leading_ff_bytes\":{},\"trailing_ff_bytes\":{},\"page_remaining\":{},\"page_landed\":{},\"page_weak\":{},\"program\":",
            self.latest,
            self.sector,
            self.latest,
            s.verdict.name(),
            s.weak_bits,
            s.weak_bytes,
            s.old_zero_bits,
            s.remaining_old_zeros,
            s.weak_at_old_zeros,
            s.new_zero_bits,
            s.landed_new_zeros,
            s.weak_at_new_zeros,
            s.ff_bytes,
            s.leading_ff_bytes,
            s.trailing_ff_bytes,
            Arr(&s.page_remaining),
            Arr(&s.page_landed),
            Arr(&s.page_weak),
        )?;
        match &s.program {
            Some(t) => write!(f, "{}", TearJson(t))?,
            None => f.write_str("null")?,
        }
        f.write_str("}")
    }
}

/// `ft-summary`: one per resumed boot, after the sectors.
pub struct SummaryRecord {
    pub latest: u32,
    pub in_flight_sector: u32,
    pub in_flight: &'static str,
    pub settled_complete: u32,
    pub settled_unexpected: u32,
    /// Sectors (any role) with at least one weak bit.
    pub weak_sectors: u32,
}

impl fmt::Display for SummaryRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{\"kind\":\"ft-summary\",\"latest\":{},\"in_flight_sector\":{},\"in_flight\":\"{}\",\"settled_complete\":{},\"settled_unexpected\":{},\"weak_sectors\":{}}}",
            self.latest,
            self.in_flight_sector,
            self.in_flight,
            self.settled_complete,
            self.settled_unexpected,
            self.weak_sectors,
        )
    }
}

/// `ft-repair`: what the boot rewrote before the work loop resumed.
pub struct RepairRecord {
    /// Region sectors rewritten with what their last cycle meant to write.
    pub sectors: u32,
    /// Journal copies erased and rewritten because they held damage.
    pub journal_copies: u32,
    /// Work cycles run to fill a fresh (or half-filled) region.
    pub init_cycles: u32,
    /// The cycle the work loop resumes at.
    pub next: u32,
}

impl fmt::Display for RepairRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{\"kind\":\"ft-repair\",\"sectors\":{},\"journal_copies\":{},\"init_cycles\":{},\"next\":{}}}",
            self.sectors, self.journal_copies, self.init_cycles, self.next,
        )
    }
}

/// `ft-timing`: one work cycle, timed, so the report can say how a cycle's
/// time divides between erase and program — which is what decides where a
/// random cut lands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimingRecord {
    pub cycle: u32,
    pub journal_us: u64,
    pub erase_us: u64,
    pub program_us: u64,
    pub page_us_min: u64,
    pub page_us_max: u64,
}

impl fmt::Display for TimingRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{\"kind\":\"ft-timing\",\"cycle\":{},\"journal_us\":{},\"erase_us\":{},\"program_us\":{},\"page_us_min\":{},\"page_us_max\":{}}}",
            self.cycle,
            self.journal_us,
            self.erase_us,
            self.program_us,
            self.page_us_min,
            self.page_us_max,
        )
    }
}

struct Opt(Option<u32>);

impl fmt::Display for Opt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(v) => write!(f, "{v}"),
            None => f.write_str("null"),
        }
    }
}

struct Arr<'a>(&'a [u16]);

impl fmt::Display for Arr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[")?;
        for (i, v) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(",")?;
            }
            write!(f, "{v}")?;
        }
        f.write_str("]")
    }
}

struct TearJson<'a>(&'a ProgramTear);

impl fmt::Display for TearJson<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let t = self.0;
        write!(
            f,
            "{{\"shape\":\"{}\",\"intended_bits\":{},\"landed_bits\":{},\"stray_bits\":{},\"prefix_bytes\":{},\"partial_bytes\":{},\"landed_extent\":{}}}",
            t.shape.name(),
            t.intended_bits,
            t.landed_bits,
            t.stray_bits,
            t.prefix_bytes,
            t.partial_bytes,
            t.landed_extent,
        )
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::format;

    use super::*;
    use crate::checks::flash_tears::analysis::{TearShape, program_tear};

    #[test]
    fn records_are_one_line_json_objects_with_a_kind() {
        let boot = format!(
            "{}",
            BootRecord {
                reset: "poweron",
                mac: [0x14, 0xc1, 0x9f, 0xe6, 0x54, 0x90],
                flash_id: 0x46_40_16,
                base: 0x35_0000,
                latest: None,
            }
        );
        assert_eq!(
            boot,
            r#"{"kind":"ft-boot","reset":"poweron","mac":"14:c1:9f:e6:54:90","flash_id":"0x464016","base":"0x350000","region_sectors":16,"latest":null,"state":"fresh"}"#
        );
        let t = program_tear(&[0x00, 0xFF], &[0x00, 0x00]);
        assert_eq!(t.shape, TearShape::BytePrefix);
        let j = format!(
            "{}",
            JournalRecord {
                copy: 1,
                scan: CopyScan {
                    valid: 3,
                    damaged: 1,
                    latest: Some(9),
                    torn_next: Some(t),
                },
            }
        );
        assert!(j.starts_with(r#"{"kind":"ft-journal","copy":1,"valid":3"#), "{j}");
        assert!(j.contains(r#""torn_next":{"shape":"byte-prefix""#), "{j}");
        assert!(j.ends_with("}}"), "{j}");
        assert!(!j.contains('\n'));
    }
}
