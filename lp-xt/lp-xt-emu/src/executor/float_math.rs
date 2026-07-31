//! Everything that computes a float value.
//!
//! Placeholder: M6 P3 fills this in behind the explicit IEEE policy layer. Until
//! then the computing half of the FP subset traps illegal rather than guessing —
//! `add.s` looks like a one-liner and is not.

use lp_xt_inst::Inst;

use crate::emu::{Emulator, Flow};
use crate::error::{EXC_ILLEGAL_INSTRUCTION, Trap, TrapKind};
use crate::trace::Tracer;

impl Emulator {
    pub(super) fn exec_float_math(
        &mut self,
        _inst: &Inst,
        _tracer: &mut dyn Tracer,
    ) -> Result<Flow, Trap> {
        Err(Trap {
            kind: TrapKind::Exception,
            cause: EXC_ILLEGAL_INSTRUCTION,
            pc: 0,
            vaddr: 0,
        })
    }
}
