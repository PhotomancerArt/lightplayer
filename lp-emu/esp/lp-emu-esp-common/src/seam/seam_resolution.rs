//! Resolution: given what a run asked for, what the scan found and the chip's
//! live address translation, decide which table is live and where to arm.
//!
//! Pure: the chip supplies `translate` (`vaddr → Some(flash offset)`, through
//! its live cache MMU) and does the arming. The rules (the plan's FD2/FD3):
//!
//! - a table is **live** when its own address translates to the flash offset
//!   it was scanned at. Two tables in flash is normal (after an update, two
//!   cores); two **live** tables is an error;
//! - a strict seam that cannot engage is a [`Outcome::StrictError`] naming
//!   both identities and what was missing; a soft one is dropped with its
//!   reason, and when nothing engages the outcome is [`Outcome::SoftNone`];
//! - the label lists only engaged atoms.
//!
//! Two passes, because a ROM-up boot cannot be judged until the app runs:
//! [`resolve_static`] at build (no table that could ever satisfy a strict
//! request is a build error), then [`resolve`] once the hart is running the
//! app and the mapping is the app's.

use super::seam_impl::SeamImpl;
use super::seam_request::{SeamRequest, Strength};
use super::seam_scan::{Candidate, ScanResult, ScannedEntry};

/// What an arm site patches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteKind {
    /// A seam function's entry: `ebreak` (or `c.ebreak`) over its first
    /// instruction.
    Code,
    /// A switch-shape seam's engaged byte: `1` over its `0`.
    EngagedByte,
}

/// One place to patch, in the firmware's address space. Where its bytes
/// live in flash is decided at arm time, through the live MMU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArmSite {
    pub imp: &'static SeamImpl,
    pub kind: SiteKind,
    pub vaddr: u32,
}

/// What engaged, against which table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Engaged {
    /// The live table.
    pub table: Candidate,
    /// In atom order.
    pub engaged: Vec<&'static SeamImpl>,
    pub sites: Vec<ArmSite>,
    /// Soft seams that did not engage, each with why.
    pub skipped: Vec<String>,
}

impl Engaged {
    /// `base` plus the engaged atoms.
    pub fn label(&self, base: &str) -> String {
        super::seam_request::label(base, &self.engaged)
    }

    /// The arm sites of one engaged seam.
    pub fn sites_of<'a>(&'a self, imp: &'a SeamImpl) -> impl Iterator<Item = &'a ArmSite> + 'a {
        self.sites
            .iter()
            .filter(move |s| s.imp.decl_id == imp.decl_id)
    }
}

/// The result of [`resolve`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Engaged(Engaged),
    /// Only soft seams were wanted, and none could engage.
    SoftNone {
        why: String,
    },
    /// A strict seam cannot engage: the run must stop.
    StrictError {
        why: String,
    },
}

/// At build, before any mapping exists: a strict request that **no** table in
/// flash could satisfy is an error now. `Ok` means "maybe": the live table is
/// chosen by [`resolve`] once the app runs.
pub fn resolve_static(request: &SeamRequest, scan: &ScanResult) -> Result<(), String> {
    for (imp, strength) in request.wanted() {
        if strength != Strength::Strict {
            continue;
        }
        if let Some(why) = scan.why_none() {
            return Err(strict_why(request, imp, &why));
        }
        let carried = scan.candidates().any(|c| entry_for(c, imp).is_ok());
        if !carried {
            let first = scan.candidates().next().expect("why_none was None");
            let why = entry_for(first, imp).unwrap_err();
            return Err(strict_why(request, imp, &why));
        }
    }
    Ok(())
}

/// Choose the live table and the arm sites. `translate` is the chip's live
/// cache MMU: `vaddr → Some(flash offset)`, `None` when unmapped.
pub fn resolve(
    request: &SeamRequest,
    scan: &ScanResult,
    translate: &dyn Fn(u32) -> Option<u32>,
) -> Outcome {
    let wanted = request.wanted();
    let strict = wanted
        .iter()
        .find(|(_, s)| *s == Strength::Strict)
        .map(|(i, _)| *i);
    let fail = |why: String| match strict {
        Some(imp) => Outcome::StrictError {
            why: strict_why(request, imp, &why),
        },
        None => Outcome::SoftNone { why },
    };

    if let Some(why) = scan.why_none() {
        return fail(why);
    }
    let live: Vec<&Candidate> = scan
        .candidates()
        .filter(|c| translate(c.self_addr) == Some(c.offset))
        .collect();
    let table = match live.as_slice() {
        [one] => (*one).clone(),
        [] => {
            let offsets: Vec<String> = scan
                .candidates()
                .map(|c| format!("{:#x} (self {:#010x})", c.offset, c.self_addr))
                .collect();
            return fail(format!(
                "no seam table is live: none of the {} in flash ({}) is where the running \
                 cache mapping puts its own address",
                offsets.len(),
                offsets.join(", ")
            ));
        }
        many => {
            let offsets: Vec<String> = many.iter().map(|c| format!("{:#x}", c.offset)).collect();
            return fail(format!(
                "{} seam tables are live at once (flash {}): the mapping is ambiguous",
                many.len(),
                offsets.join(", ")
            ));
        }
    };

    let mut engaged = Vec::new();
    let mut sites = Vec::new();
    let mut skipped = Vec::new();
    for (imp, strength) in wanted {
        match entry_for(&table, imp) {
            Ok(entry) => {
                engaged.push(imp);
                sites.push(ArmSite {
                    imp,
                    kind: SiteKind::Code,
                    vaddr: entry.function,
                });
                if entry.engaged != 0 {
                    sites.push(ArmSite {
                        imp,
                        kind: SiteKind::EngagedByte,
                        vaddr: entry.engaged,
                    });
                }
            }
            Err(why) if strength == Strength::Strict => {
                return Outcome::StrictError {
                    why: strict_why(request, imp, &why),
                };
            }
            Err(why) => skipped.push(format!("{}: {why}", imp.atom())),
        }
    }
    if engaged.is_empty() {
        return Outcome::SoftNone {
            why: skipped.join("; "),
        };
    }
    Outcome::Engaged(Engaged {
        table,
        engaged,
        sites,
        skipped,
    })
}

/// Whether the first bytes of a seam function hold its hint — `addi zero,
/// zero, <hint>` (or a compressed `c.li`/`c.addi` to `zero` with the same
/// immediate) on a halfword boundary within `bytes`. Arming checks this
/// before it patches, so a mapping that put some other function at a seam's
/// address can never be patched.
pub fn holds_seam_hint(bytes: &[u8], hint: i32) -> bool {
    let full = (((hint as u32) & 0xfff) << 20) | 0x13;
    let imm6 = (hint as u32) & 0x3f;
    let compressed = |funct3: u32| (funct3 << 13) | ((imm6 >> 5) << 12) | ((imm6 & 0x1f) << 2) | 1;
    let fits6 = (-32..32).contains(&hint);
    let mut at = 0;
    while at + 2 <= bytes.len() {
        let half = u16::from_le_bytes([bytes[at], bytes[at + 1]]) as u32;
        if half & 0b11 == 0b11 {
            if at + 4 <= bytes.len() {
                let word = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
                if word == full {
                    return true;
                }
            }
            at += 4;
        } else {
            if fits6 && (half == compressed(0b010) || half == compressed(0b000)) {
                return true;
            }
            at += 2;
        }
    }
    false
}

/// The table entry that answers `imp`, or why it cannot.
fn entry_for<'a>(table: &'a Candidate, imp: &SeamImpl) -> Result<&'a ScannedEntry, String> {
    let entry = table.entry(imp.decl_id).ok_or_else(|| {
        format!(
            "the image's seam table (flash {:#x}, firmware {}) has no entry for {} ({:#06x})",
            table.offset,
            table.version,
            imp.decl().symbol,
            imp.decl_id
        )
    })?;
    if imp.decl().shape == lp_seam::SeamShape::Switch && entry.engaged == 0 {
        return Err(format!(
            "the image's entry for {} names no engaged byte",
            imp.decl().symbol
        ));
    }
    Ok(entry)
}

fn strict_why(request: &SeamRequest, imp: &SeamImpl, why: &str) -> String {
    format!(
        "--seams {request}: {} cannot engage: {why} (emulator abi {:016x})",
        imp.atom(),
        lp_seam::SEAM_ABI_ID
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seam::seam_scan::{ScanHit, ScannedEntry};

    #[test]
    fn the_live_candidate_of_two_is_the_one_the_mapping_puts_at_its_own_address() {
        let scan = two_tables();
        // The first core is mapped at 0x4200_0000; its table sits at 0x1_0040.
        let first = |v: u32| v.checked_sub(0x4200_0000).map(|o| 0x1_0000 + o);
        let Outcome::Engaged(e) = resolve(&strict_led(), &scan, &first) else {
            panic!("engages");
        };
        assert_eq!(e.table.offset, 0x1_0040);
        assert_eq!(e.sites.len(), 1);
        assert_eq!(e.sites[0].kind, SiteKind::Code);
        assert_eq!(e.sites[0].vaddr, 0x4200_1000);
        assert_eq!(e.label("lp-emu:esp32c6:t2"), "lp-emu:esp32c6:t2+led=fast");
        // After an update the second core is mapped there instead.
        let second = |v: u32| v.checked_sub(0x4200_0000).map(|o| 0x20_0000 + o);
        let Outcome::Engaged(e) = resolve(&strict_led(), &scan, &second) else {
            panic!("engages");
        };
        assert_eq!(e.table.offset, 0x20_0040);
    }

    #[test]
    fn two_live_tables_are_refused() {
        let scan = two_tables();
        let both = |v: u32| match v {
            0x4200_0040 => Some(0x1_0040),
            _ => None,
        };
        let mut scan2 = scan.clone();
        // Make the second table claim an address that also maps to itself.
        if let ScanHit::Candidate(c) = &mut scan2.hits[1] {
            c.self_addr = 0x4300_0040;
        }
        let both2 = |v: u32| {
            both(v).or(if v == 0x4300_0040 {
                Some(0x20_0040)
            } else {
                None
            })
        };
        match resolve(&strict_led(), &scan2, &both2) {
            Outcome::StrictError { why } => assert!(why.contains("live at once"), "{why}"),
            other => panic!("{other:?}"),
        }
        match resolve(&soft_led(), &scan2, &both2) {
            Outcome::SoftNone { why } => assert!(why.contains("live at once"), "{why}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn strict_and_soft_differ_on_every_failure() {
        let unmapped = |_: u32| None;
        let cases: Vec<(ScanResult, &str)> = vec![
            (ScanResult::default(), "no seam table"),
            (
                ScanResult {
                    hits: vec![ScanHit::Mismatch {
                        offset: 0x1000,
                        abi: 7,
                    }],
                },
                "different seam declarations",
            ),
            (two_tables(), "no seam table is live"),
        ];
        for (scan, phrase) in cases {
            match resolve(&strict_led(), &scan, &unmapped) {
                Outcome::StrictError { why } => {
                    assert!(why.contains(phrase), "{why}");
                    assert!(why.contains("emulator abi"), "{why}");
                }
                other => panic!("{other:?}"),
            }
            match resolve(&soft_led(), &scan, &unmapped) {
                Outcome::SoftNone { why } => assert!(why.contains(phrase), "{why}"),
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn a_missing_entry_is_strict_error_or_soft_none() {
        let mut scan = two_tables();
        for h in &mut scan.hits {
            if let ScanHit::Candidate(c) = h {
                c.entries.clear();
            }
        }
        let first = |v: u32| v.checked_sub(0x4200_0000).map(|o| 0x1_0000 + o);
        assert!(matches!(
            resolve(&strict_led(), &scan, &first),
            Outcome::StrictError { why } if why.contains("no entry for lp_seam_ws281x_wait_step")
        ));
        assert!(matches!(
            resolve(&soft_led(), &scan, &first),
            Outcome::SoftNone { why } if why.starts_with("led=fast: ")
        ));
        assert!(resolve_static(&strict_led(), &scan).is_err());
        assert!(
            resolve_static(&soft_led(), &scan).is_ok(),
            "soft never fails a build"
        );
    }

    #[test]
    fn the_static_pass_only_refuses_what_no_table_could_satisfy() {
        assert!(resolve_static(&strict_led(), &two_tables()).is_ok());
        let err = resolve_static(&strict_led(), &ScanResult::default()).unwrap_err();
        assert!(
            err.contains("led=fast cannot engage: no seam table"),
            "{err}"
        );
    }

    #[test]
    fn the_hint_is_found_in_any_of_its_encodings() {
        // addi zero, zero, 1 ; ret
        let full = [0x13, 0x00, 0x10, 0x00, 0x82, 0x80];
        assert!(holds_seam_hint(&full, 1));
        assert!(!holds_seam_hint(&full, 2));
        // xor a0,a0,a1 (c) ; xor a0,a0,a2 (c) ; addi zero, zero, 0x701 ; ret
        let late = [0x2d, 0x8d, 0x31, 0x8d, 0x13, 0x00, 0x10, 0x70, 0x82, 0x80];
        assert!(holds_seam_hint(&late, 0x701));
        // c.li zero, 1 and c.nop 1 (c.addi zero, 1)
        assert!(holds_seam_hint(&[0x05, 0x40], 1));
        assert!(holds_seam_hint(&[0x05, 0x00], 1));
        assert!(!holds_seam_hint(&[0x82, 0x80], 1));
    }

    fn strict_led() -> SeamRequest {
        SeamRequest::strict("led=fast").unwrap()
    }

    fn soft_led() -> SeamRequest {
        SeamRequest::prefer("led=fast").unwrap()
    }

    /// Two identical cores' tables: one at flash 0x1_0040, one at
    /// 0x20_0040, both linked to live at 0x4200_0040.
    fn two_tables() -> ScanResult {
        let table = |offset| {
            ScanHit::Candidate(Candidate {
                offset,
                abi: lp_seam::SEAM_ABI_ID,
                self_addr: 0x4200_0040,
                version: "v".into(),
                pending: 0,
                entries: vec![ScannedEntry {
                    id: lp_seam::ws281x_wait_step::ID,
                    kind: Some(lp_seam::SeamKind::Performance),
                    shape: Some(lp_seam::SeamShape::Replace),
                    function: 0x4200_1000,
                    engaged: 0,
                }],
            })
        };
        ScanResult {
            hits: vec![table(0x1_0040), table(0x20_0040)],
        }
    }
}
