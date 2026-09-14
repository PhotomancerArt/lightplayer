//! The window model in wasm (XD8): the sixteen window locals against the
//! physical file, the dirty mask, the hoisted precondition, and the four
//! instructions that rotate or stage a rotate — `entry`, `retw`, `call*`
//! and `callx*` — plus `ret`.
//!
//! # The arithmetic, once
//!
//! `a_i` is `AR[(WindowBase * 4 + i) mod 64]`. Group `g` of the window
//! (`a_{4g}..a_{4g+3}`) is physical group `(WindowBase + g) mod 16`, and a
//! group never straddles the 64-word wrap, so a group's four words sit at
//! consecutive offsets from `((WindowBase + g) & 15) * 16`. The whole window
//! wraps only when `WindowBase > 12`, which is why [`Emitter::fill_window`]
//! has a fast path with sixteen constant-offset loads and a slow one that
//! masks each index.
//!
//! # `WindowStart` bit tests
//!
//! `((WindowStart << 16) | WindowStart) >> WindowBase` puts the bit for
//! frame `(WindowBase + k) mod 16` at position `k` for `k` in `0..16`, so
//! "any bit within reach of group `g`" is `(that >> 1) & ((1 << g) - 1)`
//! ([`window::overflow_in_reach`](lp_xt_emu::mach::window::overflow_in_reach)'s
//! `set(1)`, `set(2)`, `set(3)` in one mask), and shifting by `WindowBase +
//! 13` instead puts the three frames **below** the base at positions
//! `0..3` (`base-3`, `base-2`, `base-1`) for `retw`'s underflow test.

use lp_emu_core::InstClass;
use lp_xt_inst::{CallOp, CallxOp, Inst};
use wasm_encoder::{BlockType, Instruction as I};

use super::{Emitter, L_ADDR, L_CALLINC, L_DIRTY, L_T, L_T2, L_WBASE, L_WSTART, reg_local, why};
use crate::decode::{Decoded, Edges, edges};

/// The window groups a natively emitted instruction writes, as a mask over
/// the **current** window — what the block-start `L_DIRTY |=` covers.
///
/// `entry` returns 0: its one write (`a_s`, the callee's stack pointer)
/// lands in the *rotated* window and the arm marks it after the shift.
#[must_use]
pub fn written_groups(inst: &Inst) -> u8 {
    let g = |r: lp_xt_inst::Reg| 1u8 << (r.num() >> 2);
    match *inst {
        Inst::Rrr(_, rd, ..) | Inst::Rt(_, rd, _) | Inst::Rs(_, rd, _) => g(rd),
        Inst::Slli(rd, ..) | Inst::Srli(rd, ..) | Inst::Srai(rd, ..) => g(rd),
        Inst::Extui(rd, ..) | Inst::Sext(rd, ..) => g(rd),
        Inst::MovN(rt, _) | Inst::AddN(rt, ..) | Inst::AddiN(rt, ..) => g(rt),
        Inst::Addi(rt, ..) | Inst::Addmi(rt, ..) | Inst::Movi(rt, _) | Inst::MoviN(rt, _) => g(rt),
        Inst::Load(_, rt, ..) | Inst::L32iN(rt, ..) | Inst::L32r(rt, _) => g(rt),
        Inst::Call(op, _) => match op {
            CallOp::Call0 => 1,
            CallOp::Call4 => 1 << 1,
            CallOp::Call8 => 1 << 2,
            CallOp::Call12 => 1 << 3,
        },
        Inst::Callx(op, _) => match op {
            CallxOp::Callx0 => 1,
            CallxOp::Callx4 => 1 << 1,
            CallxOp::Callx8 => 1 << 2,
            CallxOp::Callx12 => 1 << 3,
        },
        _ => 0,
    }
}

impl Emitter<'_> {
    /// Where `AR[0]` sits in the imported memory.
    fn regs_at(&self, plus: u64) -> wasm_encoder::MemArg {
        self.exchange(crate::LAYOUT.regs() + plus)
    }

    /// Load the current window `a0..a15` from the physical file.
    pub(crate) fn fill_window(&mut self) {
        self.i(I::LocalGet(L_WBASE));
        self.i(I::I32Const(12));
        self.i(I::I32LeU);
        self.i(I::If(BlockType::Empty));
        // No wrap: sixteen loads at constant offsets from `WindowBase * 16`.
        self.i(I::LocalGet(L_WBASE));
        self.i(I::I32Const(4));
        self.i(I::I32Shl);
        self.i(I::LocalSet(L_T));
        for i in 0..16u8 {
            self.i(I::LocalGet(L_T));
            self.i(I::I32Load(self.regs_at(4 * u64::from(i))));
            self.i(I::LocalSet(reg_local(i)));
        }
        self.i(I::Else);
        // The window wraps past `AR[63]`: mask each index.
        for i in 0..16u8 {
            self.i(I::LocalGet(L_WBASE));
            self.i(I::I32Const(2));
            self.i(I::I32Shl);
            self.i(I::I32Const(i32::from(i)));
            self.i(I::I32Add);
            self.i(I::I32Const(63));
            self.i(I::I32And);
            self.i(I::I32Const(2));
            self.i(I::I32Shl);
            self.i(I::I32Load(self.regs_at(0)));
            self.i(I::LocalSet(reg_local(i)));
        }
        self.i(I::End);
    }

    /// `L_T = ((WindowBase + g) & 15) * 16` — the byte offset of window
    /// group `g` in the physical file.
    fn group_offset(&mut self, g: u8) {
        self.i(I::LocalGet(L_WBASE));
        if g != 0 {
            self.i(I::I32Const(i32::from(g)));
            self.i(I::I32Add);
        }
        self.i(I::I32Const(15));
        self.i(I::I32And);
        self.i(I::I32Const(4));
        self.i(I::I32Shl);
        self.i(I::LocalSet(L_T));
    }

    /// Write window group `g` back to the file **if its dirty bit is set**.
    pub(crate) fn spill_group(&mut self, g: u8) {
        self.i(I::LocalGet(L_DIRTY));
        self.i(I::I32Const(1 << g));
        self.i(I::I32And);
        self.i(I::If(BlockType::Empty));
        self.group_offset(g);
        for j in 0..4u8 {
            self.i(I::LocalGet(L_T));
            self.i(I::LocalGet(reg_local(4 * g + j)));
            self.i(I::I32Store(self.regs_at(4 * u64::from(j))));
        }
        self.i(I::End);
    }

    /// Write every dirty group back. The mask is left as it was: the
    /// caller either resets it (an escape reloads) or is leaving.
    pub(crate) fn spill_dirty(&mut self) {
        for g in 0..4u8 {
            self.spill_group(g);
        }
    }

    /// Load window group `g` from the file, unconditionally.
    fn fill_group(&mut self, g: u8) {
        self.group_offset(g);
        for j in 0..4u8 {
            self.i(I::LocalGet(L_T));
            self.i(I::I32Load(self.regs_at(4 * u64::from(j))));
            self.i(I::LocalSet(reg_local(4 * g + j)));
        }
    }

    fn store_window_words(&mut self) {
        self.store_extra(crate::extra::WINDOW_BASE, L_WBASE);
        self.store_extra(crate::extra::WINDOW_START, L_WSTART);
    }

    /// Push `((WindowStart << 16) | WindowStart) >> shift_local_plus`,
    /// with `plus` added to `WindowBase` as the shift.
    fn start_rotated(&mut self, plus: i32) {
        self.i(I::LocalGet(L_WSTART));
        self.i(I::I32Const(16));
        self.i(I::I32Shl);
        self.i(I::LocalGet(L_WSTART));
        self.i(I::I32Or);
        self.i(I::LocalGet(L_WBASE));
        if plus != 0 {
            self.i(I::I32Const(plus));
            self.i(I::I32Add);
        }
        self.i(I::I32ShrU);
    }

    /// The per-block window precondition (XD5): refuse at `block_pc` when a
    /// `WindowStart` bit lies within reach of `group` from the current base.
    pub(crate) fn window_precondition(&mut self, k: usize, block_pc: u32, group: u8) {
        if group == 0 {
            return;
        }
        self.start_rotated(0);
        self.i(I::I32Const(1));
        self.i(I::I32ShrU);
        self.i(I::I32Const((1i32 << group) - 1));
        self.i(I::I32And);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.exit(k, Some(block_pc), 0, why::WINDOW);
        self.extra -= 1;
        self.i(I::End);
    }

    /// `entry as, imm` (RM §4.7.1.4, the ENTRY page), under `woe && !excm`
    /// and `s <= 3` (the driver and [`super::refusal_of`] guarantee both):
    ///
    /// 1. the callee's frame must not overflow — `WindowCheck(00, CALLINC,
    ///    00)` with the live `CALLINC`; refuse at the `entry`'s pc if it
    ///    would;
    /// 2. rotate by `CALLINC`: the leaving groups are written back if dirty,
    ///    the staying locals slide down, the entering groups are loaded
    ///    from the file, the dirty mask slides with them;
    /// 3. the callee's `a_s` is the caller's `a_s - imm`, written in the new
    ///    window; its `WindowStart` bit is set.
    pub(crate) fn entry(&mut self, k: usize, pc: u32, s: u8, imm: u32) {
        let next = pc.wrapping_add(3);
        // 1. Bits `base+1 ..= base+CALLINC`, as a mask of `CALLINC` bits.
        self.start_rotated(0);
        self.i(I::I32Const(1));
        self.i(I::I32ShrU);
        self.i(I::I32Const(1));
        self.i(I::LocalGet(L_CALLINC));
        self.i(I::I32Shl);
        self.i(I::I32Const(1));
        self.i(I::I32Sub);
        self.i(I::I32And);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.exit(k, Some(pc), 0, why::WINDOW);
        self.extra -= 1;
        self.i(I::End);

        self.loop_commit();

        // The caller's `a_s`, read before anything moves.
        self.get(s);
        self.i(I::I32Const(imm as i32));
        self.i(I::I32Sub);
        self.i(I::LocalSet(L_T2));

        // 2. Four fixed sequences, one per `CALLINC`.
        self.br_table_on(L_CALLINC, |e, n| e.rotate_forward(n));

        // 3.
        self.i(I::LocalGet(L_T2));
        self.set(s);
        self.i(I::LocalGet(L_DIRTY));
        self.i(I::I32Const(1 << (s >> 2)));
        self.i(I::I32Or);
        self.i(I::LocalSet(L_DIRTY));
        self.i(I::LocalGet(L_WSTART));
        self.i(I::I32Const(1));
        self.i(I::LocalGet(L_WBASE));
        self.i(I::I32Shl);
        self.i(I::I32Or);
        self.i(I::LocalSet(L_WSTART));
        self.store_window_words();

        self.cycles += self.cost(InstClass::Alu);
        self.retired += 1;
        if self.loop_mark.is_some() {
            self.loop_back_next(k, next);
        } else {
            self.flush_counters();
            self.fall_through(k, next);
        }
    }

    /// The forward rotate by `n` groups: `entry`'s step 2.
    fn rotate_forward(&mut self, n: u8) {
        if n == 0 {
            return;
        }
        for g in 0..n {
            self.spill_group(g);
        }
        for i in 0..(16 - 4 * n) {
            self.i(I::LocalGet(reg_local(i + 4 * n)));
            self.i(I::LocalSet(reg_local(i)));
        }
        self.i(I::LocalGet(L_WBASE));
        self.i(I::I32Const(i32::from(n)));
        self.i(I::I32Add);
        self.i(I::I32Const(15));
        self.i(I::I32And);
        self.i(I::LocalSet(L_WBASE));
        for g in (4 - n)..4 {
            self.fill_group(g);
        }
        self.i(I::LocalGet(L_DIRTY));
        self.i(I::I32Const(i32::from(n)));
        self.i(I::I32ShrU);
        self.i(I::LocalSet(L_DIRTY));
    }

    /// The backward rotate by `n` groups: `retw`'s completion.
    fn rotate_backward(&mut self, n: u8) {
        for g in (4 - n)..4 {
            self.spill_group(g);
        }
        for i in (0..(16 - 4 * n)).rev() {
            self.i(I::LocalGet(reg_local(i)));
            self.i(I::LocalSet(reg_local(i + 4 * n)));
        }
        // The returning frame's bit is cleared before the base moves.
        self.i(I::LocalGet(L_WSTART));
        self.i(I::I32Const(1));
        self.i(I::LocalGet(L_WBASE));
        self.i(I::I32Shl);
        self.i(I::I32Const(-1));
        self.i(I::I32Xor);
        self.i(I::I32And);
        self.i(I::LocalSet(L_WSTART));
        self.i(I::LocalGet(L_WBASE));
        self.i(I::I32Const(i32::from(16 - n)));
        self.i(I::I32Add);
        self.i(I::I32Const(15));
        self.i(I::I32And);
        self.i(I::LocalSet(L_WBASE));
        for g in 0..n {
            self.fill_group(g);
        }
        self.i(I::LocalGet(L_DIRTY));
        self.i(I::I32Const(i32::from(n)));
        self.i(I::I32Shl);
        self.i(I::I32Const(15));
        self.i(I::I32And);
        self.i(I::LocalSet(L_DIRTY));
    }

    /// `block ×5 … br_table [0,1,2,3] 3` on local `sel`, with `arm(e, n)`
    /// emitted in arm `n`. The arms must not branch out — they may open
    /// their own `if`s and close them — so `extra` is untouched.
    fn br_table_on(&mut self, sel: u32, arm: impl Fn(&mut Self, u8)) {
        for _ in 0..5 {
            self.i(I::Block(BlockType::Empty));
        }
        self.i(I::LocalGet(sel));
        self.i(I::BrTable(
            alloc::borrow::Cow::Owned(alloc::vec![0, 1, 2, 3]),
            3,
        ));
        self.i(I::End);
        for n in 0..4u8 {
            arm(self, n);
            if n < 3 {
                self.i(I::Br(u32::from(3 - n)));
            }
            self.i(I::End);
        }
    }

    /// `retw` / `retw.n` (RM §4.7.1.4, the RETW page), under `woe && !excm`:
    /// the checks first — `n = a0[31:30]` must be non-zero, the caller's
    /// frame `base - n` must be resident and no closer frame may be — and
    /// any other case refuses at the `retw`'s pc for the interpreter to
    /// take (the underflow exception, or the RM's illegal-instruction
    /// reading). Then the backward rotate and the indirect lookup of
    /// `(a0 & 0x3FFF_FFFF) | (pc & 0xC000_0000)`.
    pub(crate) fn retw(&mut self, k: usize, pc: u32) {
        // n
        self.get(0);
        self.i(I::I32Const(30));
        self.i(I::I32ShrU);
        self.i(I::I32Const(3));
        self.i(I::I32And);
        self.i(I::LocalSet(L_T));
        // low = the three frames below the base at bits 0..3.
        self.start_rotated(13);
        self.i(I::I32Const(7));
        self.i(I::I32And);
        self.i(I::LocalSet(L_T2));
        // ok = (n==1 & low&4) | (n==2 & (low&6)==2) | (n==3 & (low&7)==1)
        self.i(I::LocalGet(L_T));
        self.i(I::I32Const(1));
        self.i(I::I32Eq);
        self.i(I::LocalGet(L_T2));
        self.i(I::I32Const(4));
        self.i(I::I32And);
        self.i(I::I32Const(0));
        self.i(I::I32Ne);
        self.i(I::I32And);
        self.i(I::LocalGet(L_T));
        self.i(I::I32Const(2));
        self.i(I::I32Eq);
        self.i(I::LocalGet(L_T2));
        self.i(I::I32Const(6));
        self.i(I::I32And);
        self.i(I::I32Const(2));
        self.i(I::I32Eq);
        self.i(I::I32And);
        self.i(I::I32Or);
        self.i(I::LocalGet(L_T));
        self.i(I::I32Const(3));
        self.i(I::I32Eq);
        self.i(I::LocalGet(L_T2));
        self.i(I::I32Const(1));
        self.i(I::I32Eq);
        self.i(I::I32And);
        self.i(I::I32Or);
        self.i(I::I32Eqz);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.exit(k, Some(pc), 0, why::WINDOW);
        self.extra -= 1;
        self.i(I::End);

        self.loop_commit();

        // The return pc, from the returning window's `a0`.
        self.get(0);
        self.i(I::I32Const(0x3FFF_FFFF));
        self.i(I::I32And);
        self.i(I::I32Const((pc & 0xC000_0000) as i32));
        self.i(I::I32Or);
        self.i(I::LocalSet(L_ADDR));

        self.br_table_on(L_T, |e, n| {
            if n == 0 {
                // Excluded above; said so rather than papered over.
                e.i(I::Unreachable);
            } else {
                e.rotate_backward(n);
            }
        });
        self.store_window_words();

        self.cycles += self.cost(InstClass::JalrReturn);
        self.retired += 1;
        self.flush_counters();
        self.indirect(k);
    }

    /// `ret` / `ret.n`: the non-windowed return to `a0`.
    pub(crate) fn ret(&mut self, k: usize, pc: u32) {
        let _ = pc;
        self.get(0);
        self.i(I::LocalSet(L_ADDR));
        self.loop_commit();
        self.cycles += self.cost(InstClass::JalrReturn);
        self.retired += 1;
        self.flush_counters();
        self.indirect(k);
    }

    /// The link half of a call: `a0 = pc + 3` for `call0`/`callx0`, else
    /// `a[4n] = (n << 30) | ((pc + 3) & 0x3FFF_FFFF)` in the **caller's**
    /// numbering and `PS.CALLINC = n` — in the local and in the exchange
    /// area, because a polling point can observe `PS`.
    fn link(&mut self, pc: u32, n: u8) {
        let ret = pc.wrapping_add(3);
        if n == 0 {
            self.i(I::I32Const(ret as i32));
            self.set(0);
        } else {
            let mangled = (u32::from(n) << 30) | (ret & 0x3FFF_FFFF);
            self.i(I::I32Const(mangled as i32));
            self.set(4 * n);
            self.i(I::I32Const(i32::from(n)));
            self.i(I::LocalSet(L_CALLINC));
            self.store_extra(crate::extra::PS_CALLINC, L_CALLINC);
        }
    }

    /// `call0/4/8/12`: a static target.
    pub(crate) fn call(&mut self, k: usize, pc: u32, d: &Decoded, op: CallOp) {
        let n = match op {
            CallOp::Call0 => 0,
            CallOp::Call4 => 1,
            CallOp::Call8 => 2,
            CallOp::Call12 => 3,
        };
        let Edges::Call {
            target: Some(target),
            ..
        } = edges(pc, d)
        else {
            unreachable!("a direct call names its target")
        };
        self.link(pc, n);
        self.loop_commit();
        self.cycles += self.cost(InstClass::JalCall);
        self.retired += 1;
        self.flush_counters();
        self.goto(k, target);
    }

    /// `callx0/4/8/12`: the target is read **before** the link is written
    /// (`callx8 a8` names the register it overwrites).
    pub(crate) fn callx(&mut self, k: usize, pc: u32, op: CallxOp, rs: u8) {
        let n = match op {
            CallxOp::Callx0 => 0,
            CallxOp::Callx4 => 1,
            CallxOp::Callx8 => 2,
            CallxOp::Callx12 => 3,
        };
        self.get(rs);
        self.i(I::LocalSet(L_ADDR));
        self.link(pc, n);
        self.loop_commit();
        self.cycles += self.cost(InstClass::JalrCall);
        self.retired += 1;
        self.flush_counters();
        self.indirect(k);
    }
}
