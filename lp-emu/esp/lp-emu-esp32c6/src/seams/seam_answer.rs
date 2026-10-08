//! Answering an `ebreak` at an armed seam site.
//!
//! The call shim reached the seam function with `call`, so `ra` holds the
//! return address: an answer sets `pc = ra` (the `ret` the patched body never
//! ran), counts, traces, then does what its implementation says:
//!
//! - **park** (`led=fast`): nothing more here; the run loop parks the hart
//!   until an interrupt it would wake for, exactly as after a `wfi`;
//! - **value** (`test=echo`): read `a0..a2`, write the answer to `a0`;
//! - **take** (`test=take`): copy what the endpoint holds into the buffer the
//!   call handed over — the only guest memory a seam answer ever writes;
//! - **net** (`net=lan`): one of the network seam's nine calls, by the site's
//!   declaration ([`super::net_seam`]); it too writes only buffers the call
//!   handed over.
//!
//! No answer charges a cycle (`lp_emu_esp_common::seam::seam_impl`, "What an
//! answer costs").
//!
//! Not a hook-table entry and not counted in `hook_calls`: seams have their
//! own table and their own counters.

use lp_emu_esp_common::seam::{SeamAnswer, seam_announce};

use crate::machine::Esp32C6Machine;

/// What the machine did with an `ebreak`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Served {
    /// Nobody claimed it: the guest gets the architectural breakpoint.
    No,
    /// A hook or a seam answered (or a hook stopped the run).
    Yes,
    /// Answered with `pc = ra`; now park as `wfi` would.
    Park,
}

/// What `test=echo` answers instead of silicon's `a ^ b ^ c`: the same bits
/// with a marker above them, so a test can tell whose answer ran.
pub const TEST_ECHO_MARK: u32 = 0x5ea0_0000;

impl Esp32C6Machine {
    /// An engaged seam's answer, when one claims `pc`.
    pub(crate) fn serve_seam(&mut self, pc: u32) -> Served {
        let Some(site) = self.seams.code_site_at(pc) else {
            return Served::No;
        };
        let imp = site.site.imp;
        let decl = site.site.decl;
        self.seams.calls += 1;
        if self.bus.trace.is_enabled() {
            let line = seam_announce::call_line(self.cycles(), pc, imp, decl.id);
            self.bus.trace.note(&line);
        }
        let ra = self.harts[0].regs()[1] as u32;
        self.harts[0].set_pc(ra);
        match imp.answer {
            SeamAnswer::ParkUntilInterrupt => Served::Park,
            SeamAnswer::TestEcho => {
                let r = self.harts[0].regs();
                let (a, b, c) = (r[10] as u32, r[11] as u32, r[12] as u32);
                self.harts[0].regs_mut()[10] = (TEST_ECHO_MARK | ((a ^ b ^ c) & 0xffff)) as i32;
                Served::Yes
            }
            SeamAnswer::Take => {
                let r = self.harts[0].regs();
                let (endpoint, buf, cap) = (r[10] as u32, r[11] as u32, r[12] as u32);
                let n = self.seam_take(endpoint, buf, cap);
                self.harts[0].regs_mut()[10] = n as i32;
                Served::Yes
            }
            SeamAnswer::Net => {
                let n = self.serve_net(decl.id);
                self.harts[0].regs_mut()[10] = n as i32;
                Served::Yes
            }
        }
    }

    /// `wfi`'s wake condition: an interrupt asserted and enabled in `mie`,
    /// whatever `mstatus.MIE` says (privileged spec §3.3.3).
    pub(crate) fn interrupt_wakes_hart(&self) -> bool {
        let hart = &self.harts[0];
        hart.external()
            .is_some_and(|n| hart.csr().mie & (1u32 << (n & 31)) != 0)
    }
}
