//! The words every host prints about seams, in one place, so the trace, the
//! CLI, the tab module and `emu serve` all say the same thing.
//!
//! - at every chip start, per engaged seam:
//!   `SEAM led=fast engaged (performance, abi 1f2e…, lp_seam_ws281x_wait_step@0x42001234)`
//! - at every chip start, when a soft request engaged nothing:
//!   `SEAM none engaged: <why>`
//! - per call, under `--trace`: `cyc=<n> pc=<pc> SEAM led=fast wait-step`

use lp_emu_core::sched::Cycles;

use super::seam_impl::SeamImpl;
use super::seam_resolution::{Engaged, SiteKind};

/// One line per engaged seam.
pub fn engaged_lines(engaged: &Engaged) -> Vec<String> {
    engaged
        .engaged
        .iter()
        .map(|imp| {
            let sites: Vec<String> = engaged
                .sites_of(imp)
                .map(|s| match s.kind {
                    SiteKind::Code => format!("{}@{:#010x}", imp.decl().symbol, s.vaddr),
                    SiteKind::EngagedByte => format!("engaged-byte@{:#010x}", s.vaddr),
                })
                .collect();
            engaged_line(imp, engaged.table.abi, &sites)
        })
        .collect()
}

/// `SEAM <atom> engaged (<kind>, abi <id>, <sites>)`.
pub fn engaged_line(imp: &SeamImpl, abi: u64, sites: &[String]) -> String {
    format!(
        "SEAM {} engaged ({}, abi {abi:016x}, {})",
        imp.atom(),
        imp.kind.as_str(),
        sites.join(", ")
    )
}

/// `SEAM none engaged: <why>`.
pub fn none_line(why: &str) -> String {
    format!("SEAM none engaged: {why}")
}

/// `cyc=<n> pc=<pc> SEAM <atom> <verb>`, the per-call trace line.
pub fn call_line(cycle: Cycles, pc: u32, imp: &SeamImpl) -> String {
    format!("cyc={cycle} pc={pc:#010x} SEAM {} {}", imp.atom(), imp.verb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seam::seam_impl::LED_FAST;

    #[test]
    fn the_lines_read_as_documented() {
        let line = engaged_line(
            &LED_FAST,
            0x1234,
            &["lp_seam_ws281x_wait_step@0x42001234".into()],
        );
        assert_eq!(
            line,
            "SEAM led=fast engaged (performance, abi 0000000000001234, \
             lp_seam_ws281x_wait_step@0x42001234)"
        );
        assert_eq!(
            none_line("no seam table"),
            "SEAM none engaged: no seam table"
        );
        assert_eq!(
            call_line(7, 0x4200_1234, &LED_FAST),
            "cyc=7 pc=0x42001234 SEAM led=fast wait-step"
        );
    }
}
