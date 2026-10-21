//! Answering a `break` at an armed seam site.
//!
//! The call shim reached the seam function with an ordinary windowed call
//! (`callx8` through a volatile function pointer, `lp_seam::seam_fn`'s Xtensa
//! section), and the `break` stands where the function's `entry` was, so the
//! window has **not** rotated. With `n = PS.CALLINC` (2 after `callx8`):
//!
//! - the return address is `a(4n)`, its top two bits the window increment;
//!   the answer returns to its low 30 bits under the current pc's top two —
//!   the `retw` the patched body never ran;
//! - the arguments are `a(4n+2)..`, where the callee's `a2..` would be after
//!   `entry`;
//! - the result goes to `a(4n+2)`, which is the callee's `a2` and so what the
//!   caller reads after `retw`.
//!
//! Then the answer does what its implementation says, exactly as on the C6:
//! **value** (`test=echo`), **take** (`test=take`, the only guest memory a
//! seam answer writes is the buffer the call handed over), **net**
//! (`net=lan`, [`super::net_seam`]). **Park** (`led=fast`) cannot engage on
//! this chip — no S3 image declares its seam — and would answer with a plain
//! return, which a caller that loops on its condition takes as "not yet".
//!
//! No answer charges a cycle, and none is a hook-table entry or counted in
//! `hook_calls`.

use lp_emu_esp_common::seam::{SeamAnswer, seam_announce};

use crate::machine::Machine;

/// What `test=echo` answers instead of silicon's `a ^ b ^ c`: the same bits
/// with a marker above them, so a test can tell whose answer ran. The C6's.
pub const TEST_ECHO_MARK: u32 = 0x5ea0_0000;

impl Machine {
    /// An engaged seam's answer on `core`, when one claims `pc`. `false`
    /// when no seam does: the guest gets the architectural breakpoint.
    pub(crate) fn serve_seam(&mut self, core: usize, pc: u32) -> bool {
        let Some(site) = self.seams.code_site_at(pc) else {
            return false;
        };
        let imp = site.site.imp;
        let decl = site.site.decl;
        self.seams.calls += 1;
        if self.bus.trace.is_enabled() {
            let line = seam_announce::call_line(self.cycles(), pc, imp, decl.id);
            self.bus.trace.note(&line);
        }
        let base = 4 * self.harts[core].cpu().ps_callinc;
        let ra = self.harts[core].cpu().a(base);
        self.harts[core].set_pc((ra & 0x3fff_ffff) | (pc & 0xc000_0000));
        let args = self.seam_args(core);
        let result = match imp.answer {
            SeamAnswer::ParkUntilInterrupt => return true,
            SeamAnswer::TestEcho => {
                let [a, b, c, _] = args;
                TEST_ECHO_MARK | ((a ^ b ^ c) & 0xffff)
            }
            SeamAnswer::Take => {
                let [endpoint, buf, cap, _] = args;
                self.seam_take(endpoint, buf, cap)
            }
            SeamAnswer::Net => self.serve_net(decl.id, args),
        };
        self.harts[core].cpu_mut().set_a(base + 2, result);
        true
    }

    /// The first four arguments of the call `core` stopped in, where the
    /// callee's `a2..a5` will be once `entry` rotates.
    fn seam_args(&self, core: usize) -> [u32; 4] {
        let cpu = self.harts[core].cpu();
        let base = 4 * cpu.ps_callinc + 2;
        [
            cpu.a(base),
            cpu.a(base + 1),
            cpu.a(base + 2),
            cpu.a(base + 3),
        ]
    }
}
