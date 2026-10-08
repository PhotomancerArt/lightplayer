//! The words every host prints about seams, in one place, so the trace, the
//! CLI, the tab module and `emu serve` all say the same thing.
//!
//! - at every chip start, per engaged seam:
//!   `SEAM led=fast engaged (performance, abi 1f2e…, lp_seam_ws281x_wait_step@0x42001234)`,
//!   and for a seam of several calls, their count instead of each address:
//!   `SEAM net=lan engaged (capability, abi 1f2e…, 9 sites, engaged-byte@0x42000400)`
//! - at every chip start, when a soft request engaged nothing (or a soft seam
//!   beside an engaged one did not): `SEAM none engaged: <why>`
//! - per call, under `--trace`: `cyc=<n> pc=<pc> SEAM led=fast wait-step`,
//!   `cyc=<n> pc=<pc> SEAM net=lan take-frame`

use lp_emu_core::sched::Cycles;

use super::seam_impl::SeamImpl;
use super::seam_resolution::{Engaged, SiteKind};

/// One line per engaged seam.
pub fn engaged_lines(engaged: &Engaged) -> Vec<String> {
    engaged
        .engaged
        .iter()
        .map(|imp| {
            let sites: Vec<String> = if imp.decls.len() == 1 {
                engaged
                    .sites_of(imp)
                    .map(|s| match s.kind {
                        SiteKind::Code => format!("{}@{:#010x}", s.decl.symbol, s.vaddr),
                        SiteKind::EngagedByte => format!("engaged-byte@{:#010x}", s.vaddr),
                    })
                    .collect()
            } else {
                let code = engaged
                    .sites_of(imp)
                    .filter(|s| s.kind == SiteKind::Code)
                    .count();
                std::iter::once(format!("{code} sites"))
                    .chain(
                        engaged
                            .sites_of(imp)
                            .filter(|s| s.kind == SiteKind::EngagedByte)
                            .map(|s| format!("engaged-byte@{:#010x}", s.vaddr)),
                    )
                    .collect()
            };
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

/// `cyc=<n> pc=<pc> SEAM <atom> <verb>`, the per-call trace line, for a call
/// of declaration `decl_id` ([`SeamImpl::verb_of`]).
pub fn call_line(cycle: Cycles, pc: u32, imp: &SeamImpl, decl_id: u16) -> String {
    format!(
        "cyc={cycle} pc={pc:#010x} SEAM {} {}",
        imp.atom(),
        imp.verb_of(decl_id)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seam::seam_impl::{LED_FAST, NET_LAN};

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
            call_line(7, 0x4200_1234, &LED_FAST, lp_seam::ws281x_wait_step::ID),
            "cyc=7 pc=0x42001234 SEAM led=fast wait-step"
        );
        assert_eq!(
            call_line(9, 0x4200_1000, &NET_LAN, lp_seam::net_give_frame::ID),
            "cyc=9 pc=0x42001000 SEAM net=lan give-frame"
        );
    }
}
