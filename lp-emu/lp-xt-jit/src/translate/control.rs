//! The branches, `j`, `jx`, the indirect lookup, `loop` and the loop-back.
//!
//! # The loop-back, and the interpreter's order
//!
//! The hart tests `LCOUNT != 0 && !excm && seq == LEND` **before** every
//! instruction — on the sequential next pc, against the *live* `LEND` — and
//! when it holds, decrements `LCOUNT` and makes the next pc the live `LBEG`,
//! which a taken branch or a jump then overrides (the decrement stays). An
//! instruction that aborts undoes the decrement.
//!
//! The walk marks the one instruction that can fire it — the one ending
//! exactly at a `LEND` some `loop` named (rule 6) — and the emitter
//! reproduces that order for the marked instruction only: the verdict is
//! taken before the arm ([`Emitter::loop_prelude`], into `L_LOOPED`), the
//! decrement is committed once the instruction is certain to retire
//! ([`Emitter::loop_commit`]) and written through to the exchange area
//! (a polling point may take an interrupt whose handler reads `LCOUNT`),
//! and a Next-flow instruction then branches to the live `LBEG`
//! ([`Emitter::loop_back_next`]) — through the static back-edge when the
//! live register agrees with the walk, and out of the stay
//! (`why::LOOP_BACK_MISS`) when it does not. A stay that begins with
//! `LCOUNT != 0` and a `LEND` the walk never marked cannot be exact and the
//! driver refuses it; a `loop` inside the stay sets a `LEND` the walk did
//! mark, so the marking stays complete.

use lp_emu_core::InstClass;
use lp_emu_jit::host::PERM_SHIFT;
use lp_xt_inst::{BrRi, BrRiu, BrRr, BrZ, Inst, LoopOp};
use wasm_encoder::{BlockType, Instruction as I};

use super::{
    Emitter, L_ADDR, L_CROSS, L_EXIT_PC, L_GID, L_LBEG, L_LCOUNT, L_LEND, L_LOOPED, L_NEXT, L_T,
    memarg, why,
};
use crate::decode::{Decoded, Edges, edges};

impl Emitter<'_> {
    // --- the loop-back ------------------------------------------------------

    /// `L_LOOPED = LCOUNT != 0 && LEND == next` — the verdict, taken before
    /// the marked instruction runs.
    pub(crate) fn loop_prelude(&mut self, next: u32) {
        self.i(I::LocalGet(L_LCOUNT));
        self.i(I::I32Const(0));
        self.i(I::I32Ne);
        self.i(I::LocalGet(L_LEND));
        self.i(I::I32Const(next as i32));
        self.i(I::I32Eq);
        self.i(I::I32And);
        self.i(I::LocalSet(L_LOOPED));
        self.loop_committed = false;
    }

    /// The decrement, once the marked instruction is certain to retire.
    /// Idempotent per instruction; a no-op for an unmarked one.
    pub(crate) fn loop_commit(&mut self) {
        if self.loop_mark.is_none() || self.loop_committed {
            return;
        }
        self.loop_committed = true;
        self.i(I::LocalGet(L_LOOPED));
        self.i(I::If(BlockType::Empty));
        self.i(I::LocalGet(L_LCOUNT));
        self.i(I::I32Const(1));
        self.i(I::I32Sub);
        self.i(I::LocalSet(L_LCOUNT));
        self.store_extra(crate::extra::LCOUNT, L_LCOUNT);
        self.i(I::End);
    }

    /// Undo the decrement: the marked instruction aborted after the commit
    /// (a store the bus refused).
    pub(crate) fn loop_uncommit(&mut self) {
        if self.loop_mark.is_none() || !self.loop_committed {
            return;
        }
        self.i(I::LocalGet(L_LOOPED));
        self.i(I::If(BlockType::Empty));
        self.i(I::LocalGet(L_LCOUNT));
        self.i(I::I32Const(1));
        self.i(I::I32Add);
        self.i(I::LocalSet(L_LCOUNT));
        self.store_extra(crate::extra::LCOUNT, L_LCOUNT);
        self.i(I::End);
    }

    /// Push the pc a marked instruction retires to: the live `LBEG` when the
    /// loop-back fires, `next` otherwise. A plain constant for an unmarked
    /// one.
    pub(crate) fn post_pc(&mut self, next: u32) {
        if self.loop_mark.is_some() {
            self.i(I::LocalGet(L_LBEG));
            self.i(I::I32Const(next as i32));
            self.i(I::LocalGet(L_LOOPED));
            self.i(I::Select);
        } else {
            self.i(I::I32Const(next as i32));
        }
    }

    /// Continue at the live `LBEG`: the walk's static back-edge when the
    /// register agrees with it, else out of the stay at the live value.
    fn loop_target(&mut self, k: usize) {
        let lbeg = self
            .loop_mark
            .expect("only a marked instruction loops back");
        self.i(I::LocalGet(L_LBEG));
        self.i(I::I32Const(lbeg as i32));
        self.i(I::I32Eq);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.goto(k, lbeg);
        self.i(I::Else);
        self.i(I::LocalGet(L_LBEG));
        self.i(I::LocalSet(L_ADDR));
        self.exit(k, None, 0, why::LOOP_BACK_MISS);
        self.extra -= 1;
        self.i(I::End);
    }

    /// After a Next-flow marked instruction retired natively: commit, then
    /// loop back or fall through to `next`.
    pub(crate) fn loop_back_next(&mut self, k: usize, next: u32) {
        self.loop_commit();
        self.flush_counters();
        self.i(I::LocalGet(L_LOOPED));
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.loop_target(k);
        self.extra -= 1;
        self.i(I::End);
        self.fall_through(k, next);
    }

    // --- the conditional branches -------------------------------------------

    /// Push the branch condition of `d` as an `i32`.
    fn condition(&mut self, d: &Decoded) {
        match d.inst {
            Inst::BranchRr(op, rs, rt, _) => {
                let (s, t) = (rs.num(), rt.num());
                match op {
                    BrRr::Beq | BrRr::Bne | BrRr::Blt | BrRr::Bge | BrRr::Bltu | BrRr::Bgeu => {
                        self.get(s);
                        self.get(t);
                        self.i(match op {
                            BrRr::Beq => I::I32Eq,
                            BrRr::Bne => I::I32Ne,
                            BrRr::Blt => I::I32LtS,
                            BrRr::Bge => I::I32GeS,
                            BrRr::Bltu => I::I32LtU,
                            _ => I::I32GeU,
                        });
                    }
                    // `(a & b) == b` / `!= b`
                    BrRr::Ball | BrRr::Bnall => {
                        self.get(s);
                        self.get(t);
                        self.i(I::I32And);
                        self.get(t);
                        self.i(if op == BrRr::Ball { I::I32Eq } else { I::I32Ne });
                    }
                    // `(a & b) != 0` / `== 0`
                    BrRr::Bany | BrRr::Bnone => {
                        self.get(s);
                        self.get(t);
                        self.i(I::I32And);
                        if op == BrRr::Bany {
                            self.i(I::I32Const(0));
                            self.i(I::I32Ne);
                        } else {
                            self.i(I::I32Eqz);
                        }
                    }
                    // `a & (1 << (b & 31))`; wasm masks the shift count.
                    BrRr::Bbs | BrRr::Bbc => {
                        self.get(s);
                        self.i(I::I32Const(1));
                        self.get(t);
                        self.i(I::I32Shl);
                        self.i(I::I32And);
                        if op == BrRr::Bbs {
                            self.i(I::I32Const(0));
                            self.i(I::I32Ne);
                        } else {
                            self.i(I::I32Eqz);
                        }
                    }
                }
            }
            Inst::BranchRi(op, rs, imm, _) => {
                self.get(rs.num());
                self.i(I::I32Const(imm));
                self.i(match op {
                    BrRi::Beqi => I::I32Eq,
                    BrRi::Bnei => I::I32Ne,
                    BrRi::Blti => I::I32LtS,
                    BrRi::Bgei => I::I32GeS,
                });
            }
            Inst::BranchRiu(op, rs, imm, _) => {
                self.get(rs.num());
                self.i(I::I32Const(imm));
                self.i(match op {
                    BrRiu::Bltui => I::I32LtU,
                    BrRiu::Bgeui => I::I32GeU,
                });
            }
            Inst::BranchZ(op, rs, _) => {
                self.get(rs.num());
                match op {
                    BrZ::Beqz => self.i(I::I32Eqz),
                    BrZ::Bnez => {
                        self.i(I::I32Const(0));
                        self.i(I::I32Ne);
                    }
                    BrZ::Bltz => {
                        self.i(I::I32Const(0));
                        self.i(I::I32LtS);
                    }
                    BrZ::Bgez => {
                        self.i(I::I32Const(0));
                        self.i(I::I32GeS);
                    }
                }
            }
            Inst::BranchBiI(set, rs, bit, _) => {
                self.get(rs.num());
                self.i(I::I32Const((1u32 << (bit & 31)) as i32));
                self.i(I::I32And);
                if set {
                    self.i(I::I32Const(0));
                    self.i(I::I32Ne);
                } else {
                    self.i(I::I32Eqz);
                }
            }
            Inst::BranchZN(nez, rs, _) => {
                self.get(rs.num());
                if nez {
                    self.i(I::I32Const(0));
                    self.i(I::I32Ne);
                } else {
                    self.i(I::I32Eqz);
                }
            }
            other => unreachable!("{other:?} is not a conditional branch"),
        }
    }

    /// A conditional branch: taken to its static target, not taken to the
    /// fall-through (or the loop-back, for a marked one). The decrement is
    /// committed before the flow decision, as the interpreter does.
    pub(crate) fn branch(&mut self, k: usize, pc: u32, d: &Decoded) {
        let Edges::Branch { target, next } = edges(pc, d) else {
            unreachable!("a conditional branch names its target and its fall-through")
        };
        let taken = self.cost(InstClass::BranchTaken);
        let not_taken = self.cost(InstClass::BranchNotTaken);
        // The pending charges are captured and the static accumulator
        // zeroed **before** either path is emitted: a `goto` whose target
        // is outside the set is an `exit`, and an exit hands back whatever
        // the accumulator still holds.
        let (cycles, retired) = (self.cycles, self.retired);
        self.cycles = 0;
        self.retired = 0;
        self.condition(d);
        self.i(I::LocalSet(L_T));
        self.loop_commit();
        self.i(I::LocalGet(L_T));
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.add_cycles(cycles + taken);
        self.add_retired(retired + 1);
        self.goto(k, target);
        self.extra -= 1;
        self.i(I::End);
        self.add_cycles(cycles + not_taken);
        self.add_retired(retired + 1);
        if self.loop_mark.is_some() {
            self.i(I::LocalGet(L_LOOPED));
            self.i(I::If(BlockType::Empty));
            self.extra += 1;
            self.loop_target(k);
            self.extra -= 1;
            self.i(I::End);
        }
        self.fall_through(k, next);
    }

    /// `j`.
    pub(crate) fn jump(&mut self, k: usize, pc: u32, d: &Decoded) {
        let Edges::Jump(target) = edges(pc, d) else {
            unreachable!("j names its target")
        };
        self.loop_commit();
        self.cycles += self.cost(InstClass::JalTail);
        self.retired += 1;
        self.flush_counters();
        self.goto(k, target);
    }

    /// `jx`.
    pub(crate) fn jx(&mut self, k: usize, pc: u32, rs: u8) {
        let _ = pc;
        self.get(rs);
        self.i(I::LocalSet(L_ADDR));
        self.loop_commit();
        self.cycles += self.cost(InstClass::JalrIndirect);
        self.retired += 1;
        self.flush_counters();
        self.indirect(k);
    }

    // --- indirect jumps -----------------------------------------------------

    /// Continue at the address in [`L_ADDR`], resolved through the target
    /// table: two loads, no search, no host call. **Byte granularity**
    /// (`SLOT_SHIFT = 0`): the slot index is the page offset itself, scaled
    /// by the four-byte slot. Counters are already handed back, so every
    /// path out of here leaves them where an exit would (JD17).
    pub(crate) fn indirect(&mut self, k: usize) {
        let Some(pagemap) = self.layout.indirect else {
            self.exit(k, None, 0, why::INDIRECT_NO_TABLE);
            return;
        };
        // slots = pagemap[addr >> PERM_SHIFT]; gid = slots[addr & mask]
        self.i(I::LocalGet(L_ADDR));
        self.i(I::I32Const(PERM_SHIFT as i32));
        self.i(I::I32ShrU);
        self.i(I::I32Const(2));
        self.i(I::I32Shl);
        self.i(I::I32Load(memarg(u64::from(pagemap))));
        self.i(I::LocalGet(L_ADDR));
        self.i(I::I32Const(((1u32 << PERM_SHIFT) - 1) as i32));
        self.i(I::I32And);
        self.i(I::I32Const(2));
        self.i(I::I32Shl);
        self.i(I::I32Add);
        self.i(I::I32Load(memarg(0)));
        self.i(I::LocalTee(L_GID));

        // Miss: no block starts there. Count it and leave.
        self.i(I::I32Const(-1));
        self.i(I::I32Eq);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        let miss_at = self.exchange(crate::LAYOUT.indirect_miss());
        self.i(I::I32Const(0));
        self.i(I::I32Const(0));
        self.i(I::I64Load(miss_at));
        self.i(I::I64Const(1));
        self.i(I::I64Add);
        self.i(I::I64Store(miss_at));
        self.exit(k, None, 0, why::INDIRECT_MISS);
        self.extra -= 1;
        self.i(I::End);

        // In this function: straight into the dispatcher.
        self.i(I::LocalGet(L_GID));
        self.i(I::I32Const(self.lo as i32));
        self.i(I::I32Sub);
        self.i(I::LocalTee(L_T));
        self.i(I::I32Const(self.last as i32 + 1));
        self.i(I::I32LtU);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.i(I::LocalGet(L_T));
        self.i(I::LocalSet(L_NEXT));
        self.i(I::Br(self.dispatch_depth(k)));
        self.extra -= 1;
        self.i(I::End);

        // Somewhere else in the module.
        self.i(I::LocalGet(L_GID));
        self.i(I::LocalSet(L_EXIT_PC));
        self.i(I::I32Const(1));
        self.i(I::LocalSet(L_CROSS));
        self.i(I::Br(self.exit_depth(k)));
    }

    // --- `loop` -------------------------------------------------------------

    /// `loop`, `loopnez`, `loopgtz`: `LCOUNT = as - 1`, `LBEG = pc + 3`,
    /// `LEND = pc + 4 + imm8`, written through to the exchange area (XD3's
    /// "`LOOP` is never an event": the module's own `LEND` is one the walk
    /// marked); the guarded forms skip the body. System class, so a pending
    /// yield is polled here.
    pub(crate) fn loop_inst(&mut self, k: usize, pc: u32, width: u8, op: LoopOp, rs: u8, imm8: u8) {
        let next = pc.wrapping_add(u32::from(width));
        let lend = lp_xt_inst::disasm::loop_end(pc, imm8);
        let cost = self.cost(InstClass::System);
        self.get(rs);
        self.i(I::I32Const(1));
        self.i(I::I32Sub);
        self.i(I::LocalSet(L_LCOUNT));
        self.i(I::I32Const(pc.wrapping_add(3) as i32));
        self.i(I::LocalSet(L_LBEG));
        self.i(I::I32Const(lend as i32));
        self.i(I::LocalSet(L_LEND));
        self.store_extra(crate::extra::LCOUNT, L_LCOUNT);
        self.store_extra(crate::extra::LBEG, L_LBEG);
        self.store_extra(crate::extra::LEND, L_LEND);
        let skip = match op {
            LoopOp::Loop => false,
            LoopOp::Loopnez => {
                self.get(rs);
                self.i(I::I32Eqz);
                true
            }
            LoopOp::Loopgtz => {
                self.get(rs);
                self.i(I::I32Const(0));
                self.i(I::I32LeS);
                true
            }
        };
        // The pending charges are handed back on **both** paths from the
        // same captured values: a flush inside the skip arm would reset the
        // static accumulator for the fall-through arm too.
        let (cycles, retired) = (self.cycles, self.retired);
        if skip {
            self.i(I::If(BlockType::Empty));
            self.extra += 1;
            self.pending_poll(k, lend, cost);
            // The pending charges, handed back before the `goto` (whose
            // out-of-set case is an exit that would hand them back again).
            self.cycles = 0;
            self.retired = 0;
            self.add_cycles(cycles + cost);
            self.add_retired(retired + 1);
            self.goto(k, lend);
            self.cycles = cycles;
            self.retired = retired;
            self.extra -= 1;
            self.i(I::End);
        }
        self.pending_poll(k, next, cost);
        self.cycles = 0;
        self.retired = 0;
        self.add_cycles(cycles + cost);
        self.add_retired(retired + 1);
        self.fall_through(k, next);
    }
}
