//! Miscellaneous zero-operand executors: barriers, nops, and the illegal trap.

use lp_xt_inst::{Inst, NullaryNarrowOp, NullaryOp};

use crate::emu::{Emulator, Flow};
use crate::error::{EXC_ILLEGAL_INSTRUCTION, Trap, TrapKind};

impl Emulator {
    pub(super) fn exec_misc(&mut self, inst: &Inst, pc: u32) -> Result<Flow, Trap> {
        match *inst {
            // Barriers / sync / nop: no architectural effect in this model.
            Inst::Nullary(
                NullaryOp::Memw
                | NullaryOp::Extw
                | NullaryOp::Isync
                | NullaryOp::Rsync
                | NullaryOp::Esync
                | NullaryOp::Dsync
                | NullaryOp::Nop,
            )
            | Inst::NullaryN(NullaryNarrowOp::NopN) => Ok(Flow::Next),

            // SYSCALL: surface to the run loop, which dispatches to the host
            // [`SyscallHandler`](crate::SyscallHandler) (or raises SyscallCause
            // when none is installed, as unhandled hardware would).
            Inst::Nullary(NullaryOp::Syscall) => Ok(Flow::Syscall),

            // ILL / ILL.N: raise an illegal-instruction exception.
            Inst::Nullary(NullaryOp::Ill) | Inst::NullaryN(NullaryNarrowOp::IllN) => Err(Trap {
                kind: TrapKind::Exception,
                cause: EXC_ILLEGAL_INSTRUCTION,
                pc,
                vaddr: 0,
            }),

            // RET/RETW are routed in mod.rs; anything else here is unexpected.
            _ => unreachable!("exec_misc got {inst:?}"),
        }
    }
}
