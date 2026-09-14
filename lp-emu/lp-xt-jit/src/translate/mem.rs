//! `l32r`, the plain loads and stores, and the polling points a store and a
//! System-class instruction owe.
//!
//! # The permission byte, and what the classic adds to it
//!
//! The ABI's table says, per 16 KiB page, [`PERM_NONE`] (every access goes
//! to the bus), [`PERM_READ`] (loads inline, stores to the bus) or
//! [`PERM_READ_WRITE`] (both inline). This machine's SRAM0 fits none of
//! them: its bus takes **aligned 32-bit data accesses and nothing else**
//! (`AccessRule::WordOnly`, DD37), and it is the executable region the
//! guest publishes its shader into — so a store there is the invalidation
//! event (XD3) and the bus has to see it. [`PERM_READ_WORD`] is the fourth
//! value, written by the classic's driver for exactly those pages: an
//! aligned word load is inline, every other access is the bus's.
//!
//! The fast paths, by width:
//!
//! | access | inline when |
//! |---|---|
//! | word load | `addr & 3 == 0` and the byte is non-zero (an aligned word never straddles a page) |
//! | halfword load | both ends' bytes equal and in `{READ, READ_WRITE}` |
//! | byte load | the byte in `{READ, READ_WRITE}` |
//! | word store | `addr & 3 == 0` and the byte is `READ_WRITE` |
//! | halfword store | both ends' bytes equal `READ_WRITE` |
//! | byte store | the byte is `READ_WRITE` |
//!
//! Everything else — MMIO, a misaligned word, a sub-word access on SRAM0, a
//! straddle, an unmapped address — goes out through the import, where the
//! bus serves it exactly as it serves the interpreter, and a refused access
//! leaves at the instruction's own pc with nothing retired. There is no
//! straddle exit: the bus is the oracle for every case the fast path does
//! not take.

use lp_emu_core::InstClass;
use lp_emu_jit::host::{
    FLAG_PENDING, FLAG_SLICE_ENDED, MMIO_PENDING, MMIO_REFUSED, MMIO_SLICE_ENDED, PERM_READ_WRITE,
    PERM_SHIFT, load_kind, store_kind,
};
use lp_emu_jit::translate::{F_MMIO_STORE, F_POLL};
use lp_xt_inst::{LoadOp, StoreOp};
use wasm_encoder::{BlockType, Instruction as I, ValType};

use super::{
    Emitter, L_ADDR, L_CYC, L_INSTRET, L_PENDING, L_PERM_HI, L_PERM_LO, L_STATUS, L_T, L_T64,
    P_WATCH_HI, P_WATCH_LO, memarg, why,
};

/// Permission byte: plain RAM for **aligned word loads only**. Every other
/// access — a sub-word or misaligned load, any store — goes out through the
/// import. The classic's SRAM0 (word-only, executable, the store-address
/// invalidation event's home).
pub const PERM_READ_WORD: u8 = 3;

/// A load's shape: one of the four `RRI8` loads, or the narrow `l32i.n`.
#[derive(Clone, Copy)]
pub(crate) enum LoadShape {
    Op(LoadOp),
    Word,
}

/// A store's shape: one of the three `RRI8` stores, or the narrow `s32i.n`.
#[derive(Clone, Copy)]
pub(crate) enum StoreShape {
    Op(StoreOp),
    Word,
}

impl Emitter<'_> {
    fn arena(&self) -> wasm_encoder::MemArg {
        memarg(u64::from(self.layout.arena_offset))
    }

    fn perm_byte(&mut self, offset: u32) {
        self.i(I::LocalGet(L_ADDR));
        if offset != 0 {
            self.i(I::I32Const(offset as i32));
            self.i(I::I32Add);
        }
        self.i(I::I32Const(PERM_SHIFT as i32));
        self.i(I::I32ShrU);
        self.i(I::I32Load8U(memarg(u64::from(self.layout.perm_offset))));
    }

    /// The address in [`L_ADDR`] as an offset into the imported memory.
    fn guest_offset(&mut self) {
        self.i(I::LocalGet(L_ADDR));
        self.i(I::I32Const(self.layout.guest_base as i32));
        self.i(I::I32Sub);
    }

    /// Push the fast-path predicate for a `width`-byte access; `store` asks
    /// for `READ_WRITE`, a load for "loads inline" — which for a sub-word
    /// access excludes [`PERM_READ_WORD`].
    fn fast_predicate(&mut self, width: u32, store: bool) {
        self.perm_byte(0);
        self.i(I::LocalSet(L_PERM_LO));
        match width {
            4 => {
                self.i(I::LocalGet(L_ADDR));
                self.i(I::I32Const(3));
                self.i(I::I32And);
                self.i(I::I32Eqz);
                self.i(I::LocalGet(L_PERM_LO));
                if store {
                    self.i(I::I32Const(i32::from(PERM_READ_WRITE)));
                    self.i(I::I32Eq);
                } else {
                    self.i(I::I32Const(0));
                    self.i(I::I32Ne);
                }
                self.i(I::I32And);
            }
            2 => {
                self.perm_byte(1);
                self.i(I::LocalSet(L_PERM_HI));
                self.i(I::LocalGet(L_PERM_LO));
                self.i(I::LocalGet(L_PERM_HI));
                self.i(I::I32Eq);
                self.i(I::LocalGet(L_PERM_LO));
                self.sub_word_ok(store);
                self.i(I::I32And);
            }
            _ => {
                self.i(I::LocalGet(L_PERM_LO));
                self.sub_word_ok(store);
            }
        }
    }

    /// With the byte on the stack: `== READ_WRITE` for a store, `in {READ,
    /// READ_WRITE}` (`(b - 1) <u 2`) for a load.
    fn sub_word_ok(&mut self, store: bool) {
        if store {
            self.i(I::I32Const(i32::from(PERM_READ_WRITE)));
            self.i(I::I32Eq);
        } else {
            self.i(I::I32Const(1));
            self.i(I::I32Sub);
            self.i(I::I32Const(2));
            self.i(I::I32LtU);
        }
    }

    /// The import path of a load: `mmio_load(pc, cycle, addr, kind)`,
    /// leaving the value on the stack, or exiting at `pc` when the bus
    /// refused. The counters handed over are the pre-instruction ones.
    fn load_import(&mut self, k: usize, pc: u32, kind_code: u32) {
        self.i(I::I32Const(pc as i32));
        self.i(I::LocalGet(L_CYC));
        if self.cycles != 0 {
            self.i(I::I64Const(self.cycles as i64));
            self.i(I::I64Add);
        }
        self.i(I::LocalGet(L_ADDR));
        self.i(I::I32Const(kind_code as i32));
        self.i(I::Call(self.mmio_load_func));
        self.i(I::LocalSet(L_T64));
        self.i(I::LocalGet(L_T64));
        self.i(I::I64Const(32));
        self.i(I::I64ShrU);
        self.i(I::I32WrapI64);
        self.i(I::LocalSet(L_STATUS));
        self.i(I::LocalGet(L_STATUS));
        self.i(I::I32Const(MMIO_REFUSED as i32));
        self.i(I::I32Eq);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        // The access did not happen. Leave at this instruction, having
        // retired nothing of it, and let the interpreter take the trap.
        self.exit(k, Some(pc), 0, why::LOAD_REFUSED);
        self.extra -= 1;
        self.i(I::End);
        self.i(I::LocalGet(L_T64));
        self.i(I::I32WrapI64);
    }

    /// After a load: the bus may be holding a yield now. Nothing happens
    /// here — the interpreter does not look after a load either — but the
    /// next store or System-class instruction has to poll.
    fn note_pending(&mut self) {
        self.i(I::LocalGet(L_STATUS));
        self.i(I::I32Const(MMIO_PENDING as i32));
        self.i(I::I32Eq);
        self.i(I::If(BlockType::Empty));
        self.i(I::I32Const(FLAG_PENDING));
        self.i(I::LocalSet(L_PENDING));
        self.i(I::End);
    }

    /// `l8ui`, `l16ui`, `l16si`, `l32i`, `l32i.n`.
    pub(crate) fn load(&mut self, k: usize, pc: u32, shape: LoadShape, rt: u8, rs: u8, off: u32) {
        let (width, kind_code, ins) = match shape {
            LoadShape::Op(LoadOp::L8ui) => (1u32, load_kind::BU, I::I32Load8U(self.arena())),
            LoadShape::Op(LoadOp::L16ui) => (2, load_kind::HU, I::I32Load16U(self.arena())),
            LoadShape::Op(LoadOp::L16si) => (2, load_kind::H, I::I32Load16S(self.arena())),
            LoadShape::Op(LoadOp::L32i) | LoadShape::Word => {
                (4, load_kind::W, I::I32Load(self.arena()))
            }
        };
        self.i(I::I32Const(0));
        self.i(I::LocalSet(L_STATUS));
        self.get(rs);
        self.i(I::I32Const(off as i32));
        self.i(I::I32Add);
        self.i(I::LocalSet(L_ADDR));
        self.fast_predicate(width, false);
        self.i(I::If(BlockType::Result(ValType::I32)));
        self.extra += 1;
        self.guest_offset();
        self.i(ins);
        self.i(I::Else);
        self.load_import(k, pc, kind_code);
        self.i(I::End);
        self.extra -= 1;
        self.set(rt);
        self.note_pending();
    }

    /// `l32r`: the address is `((pc + 3) & !3) + ((field - 0x1_0000) << 2)`,
    /// a compile-time constant and word-aligned, so the fast test is one
    /// constant-offset byte load.
    pub(crate) fn l32r(&mut self, k: usize, pc: u32, rt: u8, field: u16) {
        let base = pc.wrapping_add(3) & !3;
        let off = ((i32::from(field) - 0x1_0000) << 2) as u32;
        let addr = base.wrapping_add(off);
        self.i(I::I32Const(0));
        self.i(I::LocalSet(L_STATUS));
        self.i(I::I32Const(addr as i32));
        self.i(I::LocalSet(L_ADDR));
        self.i(I::I32Const(0));
        self.i(I::I32Load8U(memarg(
            u64::from(self.layout.perm_offset) + u64::from(addr >> PERM_SHIFT),
        )));
        self.i(I::If(BlockType::Result(ValType::I32)));
        self.extra += 1;
        self.i(I::I32Const(addr.wrapping_sub(self.layout.guest_base) as i32));
        self.i(I::I32Load(self.arena()));
        self.i(I::Else);
        self.load_import(k, pc, load_kind::W);
        self.i(I::End);
        self.extra -= 1;
        self.set(rt);
        self.note_pending();
    }

    /// `s8i`, `s16i`, `s32i`, `s32i.n`.
    #[expect(
        clippy::too_many_arguments,
        reason = "the instruction's operands, its width and the block context"
    )]
    pub(crate) fn store(
        &mut self,
        k: usize,
        pc: u32,
        inst_width: u8,
        shape: StoreShape,
        rt: u8,
        rs: u8,
        off: u32,
    ) {
        let next = pc.wrapping_add(u32::from(inst_width));
        let cost = self.cost(InstClass::Store);
        let (width, kind_code, ins) = match shape {
            StoreShape::Op(StoreOp::S8i) => (1u32, store_kind::B, I::I32Store8(self.arena())),
            StoreShape::Op(StoreOp::S16i) => (2, store_kind::H, I::I32Store16(self.arena())),
            StoreShape::Op(StoreOp::S32i) | StoreShape::Word => {
                (4, store_kind::W, I::I32Store(self.arena()))
            }
        };
        self.i(I::I32Const(0));
        self.i(I::LocalSet(L_STATUS));
        self.get(rs);
        self.i(I::I32Const(off as i32));
        self.i(I::I32Add);
        self.i(I::LocalSet(L_ADDR));

        // The one store watchpoint the bus can hand over, checked exactly as
        // the bus does: a hit when `a < hi && lo < a + len`, in 64 bits,
        // before anything else. `lo == hi == 0` is "nothing armed".
        self.i(I::LocalGet(L_ADDR));
        self.i(I::I64ExtendI32U);
        self.i(I::LocalGet(P_WATCH_HI));
        self.i(I::I64LtU);
        self.i(I::LocalGet(P_WATCH_LO));
        self.i(I::LocalGet(L_ADDR));
        self.i(I::I64ExtendI32U);
        self.i(I::I64Const(i64::from(width)));
        self.i(I::I64Add);
        self.i(I::I64LtU);
        self.i(I::I32And);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.exit(k, Some(pc), 0, why::STORE_PERM);
        self.extra -= 1;
        self.i(I::End);

        self.fast_predicate(width, true);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        // Inline. The bus never sees it, so the only thing polling point (c)
        // can have to do is the obligation an earlier load left.
        self.guest_offset();
        self.get(rt);
        self.i(ins);
        self.loop_commit();
        self.pending_poll(k, next, cost);
        self.i(I::Else);
        // The bus serves the store and runs the polling point in the same
        // crossing: the store's own `(pc, cycle)` first, then the post-store
        // state the hart would be holding when it polled. The loop-back's
        // decrement is part of that state (the handler's `save_context`
        // reads `LCOUNT`), so it is committed first and undone if the bus
        // refused the store.
        self.loop_commit();
        self.i(I::I32Const(pc as i32));
        self.i(I::LocalGet(L_CYC));
        if self.cycles != 0 {
            self.i(I::I64Const(self.cycles as i64));
            self.i(I::I64Add);
        }
        self.i(I::LocalGet(L_ADDR));
        self.i(I::I32Const(kind_code as i32));
        self.get(rt);
        self.post_store_state(next, cost);
        self.i(I::Call(F_MMIO_STORE));
        self.unpack_poll();
        self.i(I::LocalGet(L_STATUS));
        self.i(I::I32Const(MMIO_REFUSED as i32));
        self.i(I::I32Eq);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.loop_uncommit();
        self.exit(k, Some(pc), 0, why::STORE_REFUSED);
        self.extra -= 1;
        self.i(I::End);
        // The store retired and the host polled, so whatever the bus was
        // holding is taken.
        self.i(I::I32Const(0));
        self.i(I::LocalSet(L_PENDING));
        self.i(I::LocalGet(L_STATUS));
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.exit_after_poll(k, cost);
        self.extra -= 1;
        self.i(I::End);
        self.i(I::End);
        self.extra -= 1;
    }

    /// Polling point (c) for an instruction the bus never saw — an inline
    /// store, or a System-class no-op — owed only when an earlier load left
    /// a yield ([`L_PENDING`]). `cost` is this instruction's; the poll sees
    /// the post-instruction state.
    pub(crate) fn pending_poll(&mut self, k: usize, next: u32, cost: u64) {
        self.i(I::LocalGet(L_PENDING));
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.i(I::I32Const(0));
        self.i(I::LocalSet(L_PENDING));
        self.post_store_state(next, cost);
        self.i(I::Call(F_POLL));
        self.unpack_poll();
        self.i(I::LocalGet(L_STATUS));
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.exit_after_poll(k, cost);
        self.extra -= 1;
        self.i(I::End);
        self.extra -= 1;
        self.i(I::End);
    }

    /// Leave because polling point (c) said so, at the pc it left the hart
    /// on ([`L_ADDR`]), with this instruction charged and retired.
    ///
    /// [`MMIO_SLICE_ENDED`] is the bus taking the slice; anything else is the
    /// hart having moved (an interrupt's vector), the bus no longer pure, or
    /// the store having published code — the stay leaves with **no** flag,
    /// because the polling point has already run.
    fn exit_after_poll(&mut self, k: usize, cost: u64) {
        self.i(I::LocalGet(L_STATUS));
        self.i(I::I32Const(MMIO_SLICE_ENDED as i32));
        self.i(I::I32Eq);
        self.i(I::LocalSet(L_T));
        self.i(I::I32Const(why::SLICE_ENDED));
        self.i(I::I32Const(why::AFTER_STORE));
        self.i(I::LocalGet(L_T));
        self.i(I::Select);
        self.i(I::LocalSet(super::L_WHY));
        self.i(I::I32Const(FLAG_SLICE_ENDED));
        self.i(I::I32Const(0));
        self.i(I::LocalGet(L_T));
        self.i(I::Select);
        self.i(I::LocalSet(super::L_FLAGS));
        let (c, r) = (self.cycles + cost, self.retired + 1);
        self.add_cycles(c);
        self.add_retired(r);
        self.i(I::LocalGet(L_ADDR));
        self.i(I::LocalSet(super::L_EXIT_PC));
        self.i(I::Br(self.exit_depth(k)));
    }

    /// Push the three post-instruction arguments every polling point takes:
    /// the pc the instruction retires to (the loop-back's `LBEG` when this
    /// instruction is a marked loop end and the loop-back fires), and the
    /// counters with this instruction charged (JD17).
    fn post_store_state(&mut self, next: u32, cost: u64) {
        self.post_pc(next);
        self.i(I::LocalGet(L_CYC));
        self.i(I::I64Const((self.cycles + cost) as i64));
        self.i(I::I64Add);
        self.i(I::LocalGet(L_INSTRET));
        self.i(I::I64Const(i64::from(self.retired) + 1));
        self.i(I::I64Add);
    }

    /// Unpack a polling point's `(status << 32) | pc` into [`L_STATUS`] and
    /// [`L_ADDR`].
    fn unpack_poll(&mut self) {
        self.i(I::LocalSet(L_T64));
        self.i(I::LocalGet(L_T64));
        self.i(I::I64Const(32));
        self.i(I::I64ShrU);
        self.i(I::I32WrapI64);
        self.i(I::LocalSet(L_STATUS));
        self.i(I::LocalGet(L_T64));
        self.i(I::I32WrapI64);
        self.i(I::LocalSet(L_ADDR));
    }
}
