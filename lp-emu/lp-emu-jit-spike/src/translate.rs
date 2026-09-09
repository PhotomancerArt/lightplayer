//! spike: one hot region → one wasm module.
//!
//! Shape (see the planning doc): guest registers live in wasm locals for the
//! length of a stay in the region; guest memory is the imported linear memory
//! the bus itself now aliases (`GUEST_BASE` maps to offset 0); RAM loads and
//! stores are inline after a 16 KiB-page permission check; anything that is
//! not plain RAM goes through the `mmio_load` / `mmio_store` imports with the
//! exact `(pc, cycle)` the interpreter would have set; the slice budget is
//! checked once per block against the block's exact maximum cost, and every
//! exit reports the pc, the cycle count and the retired-instruction delta so
//! the interpreter resumes with identical state.
//!
//! Control flow inside the region is a dispatcher loop with one `br_table`
//! over the region's blocks; forward edges branch straight to the target's
//! block label, back edges go through the table. Indirect jumps compare
//! against the targets the census observed and otherwise exit.

use std::borrow::Cow;
use std::collections::HashMap;

use lp_emu_core::{CycleModel, InstClass};
use wasm_encoder::{
    BlockType, CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection,
    ImportSection, Instruction as I, MemArg, MemoryType, Module, TypeSection, ValType,
};

use crate::decode::{Cond, Decoded, Inst, LoadKind, Op, OpI, StoreKind};

/// Guest address that maps to linear-memory offset 0.
pub const GUEST_BASE: u32 = 0x4000_0000;
/// Host<->region exchange area: regs[32] at +0, cycle (i64) at +128, ran
/// (i32) at +136, flag (i32) at +140.
pub const SCRATCH: u32 = 0x1001_0000;
/// One byte per 16 KiB page of the whole 4 GiB guest space: 0 = not plain
/// RAM (import), 1 = readable, 2 = readable and writable.
pub const PERM: u32 = 0x1002_0000;
pub const PERM_SHIFT: u32 = 14;
/// 0x1006_0000 bytes: every region, the scratch page and the permission table.
pub const PAGES: u64 = 0x1006_0000 / 65536;

pub enum BlockEnd {
    /// The last instruction is the control transfer.
    Term,
    /// Falls into `pc` (the next listed block start).
    Fall(u32),
    /// Leaves the region before `pc` (an instruction the translator refuses).
    Undecodable(u32),
}

pub struct BlockCode {
    pub pc: u32,
    pub insts: Vec<(u32, Decoded)>,
    pub end: BlockEnd,
    pub bytes: u32,
}

pub struct RegionCode {
    /// Sorted by pc.
    pub blocks: Vec<BlockCode>,
    pub index: HashMap<u32, usize>,
    /// Indirect-jump targets to dispatch internally, hottest first.
    pub targets: Vec<u32>,
}

// Local indices.
const P_ENTRY: u32 = 0;
const P_CYC0: u32 = 1;
const P_END: u32 = 2;
const fn reg_local(r: u8) -> u32 {
    2 + r as u32
}
const CYC: u32 = 34;
const RAN: u32 = 35;
const EXIT_PC: u32 = 36;
const FLAG: u32 = 37;
const NEXT: u32 = 38;
const A: u32 = 39;
const P: u32 = 40;
const Q: u32 = 41;
const ST: u32 = 42;
const T64: u32 = 43;
const T: u32 = 44;
const WLO: u32 = 45;
const WHI: u32 = 46;

const F_MMIO_LOAD: u32 = 0;
const F_MMIO_STORE: u32 = 1;

fn memarg(offset: u64) -> MemArg {
    MemArg {
        offset,
        align: 0,
        memory_index: 0,
    }
}

struct Emitter<'a> {
    f: Function,
    code: &'a RegionCode,
    model: CycleModel,
    n: usize,
    /// Nesting added by `if`s inside the current block body.
    extra: u32,
}

impl Emitter<'_> {
    fn i(&mut self, ins: I<'static>) {
        self.f.instruction(&ins);
    }
    fn cost(&self, class: InstClass) -> u64 {
        u64::from(self.model.cycles_for(class))
    }
    fn get(&mut self, r: u8) {
        if r == 0 {
            self.i(I::I32Const(0));
        } else {
            self.i(I::LocalGet(reg_local(r)));
        }
    }
    fn set(&mut self, r: u8) {
        if r == 0 {
            self.i(I::Drop);
        } else {
            self.i(I::LocalSet(reg_local(r)));
        }
    }
    fn exit_depth(&self, k: usize) -> u32 {
        (self.n - k) as u32 + 1 + self.extra
    }
    fn dispatch_depth(&self, k: usize) -> u32 {
        (self.n - k) as u32 + self.extra
    }
    fn body_depth(&self, k: usize, j: usize) -> u32 {
        (j - k - 1) as u32 + self.extra
    }

    fn add_cyc(&mut self, pending: u64) {
        if pending != 0 {
            self.i(I::LocalGet(CYC));
            self.i(I::I64Const(pending as i64));
            self.i(I::I64Add);
            self.i(I::LocalSet(CYC));
        }
    }
    fn add_ran(&mut self, d: u32) {
        if d != 0 {
            self.i(I::LocalGet(RAN));
            self.i(I::I32Const(d as i32));
            self.i(I::I32Add);
            self.i(I::LocalSet(RAN));
        }
    }

    /// Leave the region: `pc` (or the address in `A` when `None`), after
    /// charging `pending` cycles and `ran` instructions.
    fn exit(&mut self, k: usize, pc: Option<u32>, pending: u64, ran: u32, flag: i32) {
        self.add_cyc(pending);
        self.add_ran(ran);
        match pc {
            Some(pc) => self.i(I::I32Const(pc as i32)),
            None => self.i(I::LocalGet(A)),
        }
        self.i(I::LocalSet(EXIT_PC));
        if flag != 0 {
            self.i(I::I32Const(flag));
            self.i(I::LocalSet(FLAG));
        }
        self.i(I::Br(self.exit_depth(k)));
    }

    /// Continue at `target` (counters already charged).
    fn goto(&mut self, k: usize, target: u32) {
        match self.code.index.get(&target).copied() {
            Some(j) if j > k => self.i(I::Br(self.body_depth(k, j))),
            Some(j) => {
                self.i(I::I32Const(j as i32));
                self.i(I::LocalSet(NEXT));
                self.i(I::Br(self.dispatch_depth(k)));
            }
            None => self.exit(k, Some(target), 0, 0, 0),
        }
    }

    fn perm_check(&mut self, width: u32) {
        // P = perm[A >> 14]; Q = perm[(A + width - 1) >> 14]
        self.i(I::LocalGet(A));
        self.i(I::I32Const(PERM_SHIFT as i32));
        self.i(I::I32ShrU);
        self.i(I::I32Load8U(memarg(PERM as u64)));
        self.i(I::LocalSet(P));
        if width > 1 {
            self.i(I::LocalGet(A));
            self.i(I::I32Const(width as i32 - 1));
            self.i(I::I32Add);
            self.i(I::I32Const(PERM_SHIFT as i32));
            self.i(I::I32ShrU);
            self.i(I::I32Load8U(memarg(PERM as u64)));
            self.i(I::LocalSet(Q));
        } else {
            self.i(I::LocalGet(P));
            self.i(I::LocalSet(Q));
        }
    }

    fn guest_offset(&mut self) {
        self.i(I::LocalGet(A));
        self.i(I::I32Const(GUEST_BASE as i32));
        self.i(I::I32Sub);
    }

    #[allow(clippy::too_many_arguments)]
    fn load(
        &mut self,
        k: usize,
        pc: u32,
        width: u8,
        cost: u64,
        pending: u64,
        j: u32,
        kind: LoadKind,
        rd: u8,
        rs1: u8,
        imm: i32,
    ) {
        let (w, kind_code) = match kind {
            LoadKind::B => (1, 0),
            LoadKind::H => (2, 1),
            LoadKind::W => (4, 2),
            LoadKind::Bu => (1, 4),
            LoadKind::Hu => (2, 5),
        };
        self.i(I::I32Const(0));
        self.i(I::LocalSet(ST));
        self.get(rs1);
        self.i(I::I32Const(imm));
        self.i(I::I32Add);
        self.i(I::LocalSet(A));
        self.perm_check(w);
        // fast: P == Q && P != 0
        self.i(I::LocalGet(P));
        self.i(I::LocalGet(Q));
        self.i(I::I32Eq);
        self.i(I::LocalGet(P));
        self.i(I::I32Const(0));
        self.i(I::I32Ne);
        self.i(I::I32And);
        self.i(I::If(BlockType::Result(ValType::I32)));
        self.extra += 1;
        self.guest_offset();
        self.i(match kind {
            LoadKind::B => I::I32Load8S(memarg(0)),
            LoadKind::H => I::I32Load16S(memarg(0)),
            LoadKind::W => I::I32Load(memarg(0)),
            LoadKind::Bu => I::I32Load8U(memarg(0)),
            LoadKind::Hu => I::I32Load16U(memarg(0)),
        });
        self.i(I::Else);
        self.i(I::LocalGet(P));
        self.i(I::LocalGet(Q));
        self.i(I::I32Or);
        self.i(I::I32Eqz);
        self.i(I::If(BlockType::Result(ValType::I32)));
        self.extra += 1;
        self.i(I::I32Const(pc as i32));
        self.i(I::LocalGet(CYC));
        self.i(I::I64Const(pending as i64));
        self.i(I::I64Add);
        self.i(I::LocalGet(A));
        self.i(I::I32Const(kind_code));
        self.i(I::Call(F_MMIO_LOAD));
        self.i(I::LocalSet(T64));
        self.i(I::LocalGet(T64));
        self.i(I::I64Const(32));
        self.i(I::I64ShrU);
        self.i(I::I32WrapI64);
        self.i(I::LocalSet(ST));
        self.i(I::LocalGet(ST));
        self.i(I::I32Const(1));
        self.i(I::I32Eq);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.exit(k, Some(pc), pending, j, 0);
        self.extra -= 1;
        self.i(I::End);
        self.i(I::LocalGet(T64));
        self.i(I::I32WrapI64);
        self.i(I::Else);
        self.exit(k, Some(pc), pending, j, 0);
        self.i(I::I32Const(0));
        self.i(I::End);
        self.extra -= 1;
        self.i(I::End);
        self.extra -= 1;
        self.set(rd);
        // status 2: the access raised a side-band or a yield; leave after it.
        self.i(I::LocalGet(ST));
        self.i(I::I32Const(2));
        self.i(I::I32Eq);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.exit(k, Some(pc.wrapping_add(u32::from(width))), pending + cost, j + 1, 0);
        self.extra -= 1;
        self.i(I::End);
    }

    #[allow(clippy::too_many_arguments)]
    fn store(
        &mut self,
        k: usize,
        pc: u32,
        width: u8,
        cost: u64,
        pending: u64,
        j: u32,
        kind: StoreKind,
        rs1: u8,
        rs2: u8,
        imm: i32,
    ) {
        let (w, kind_code) = match kind {
            StoreKind::B => (1, 0),
            StoreKind::H => (2, 1),
            StoreKind::W => (4, 2),
        };
        self.i(I::I32Const(0));
        self.i(I::LocalSet(ST));
        self.get(rs1);
        self.i(I::I32Const(imm));
        self.i(I::I32Add);
        self.i(I::LocalSet(A));
        // The store watchpoint, exactly as `check_store_watchpoint`: a hit
        // when `a0 < hi && lo < a0 + len`, in 64-bit, before anything else.
        self.i(I::LocalGet(A));
        self.i(I::I64ExtendI32U);
        self.i(I::LocalGet(WHI));
        self.i(I::I64LtU);
        self.i(I::LocalGet(WLO));
        self.i(I::LocalGet(A));
        self.i(I::I64ExtendI32U);
        self.i(I::I64Const(i64::from(w)));
        self.i(I::I64Add);
        self.i(I::I64LtU);
        self.i(I::I32And);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.exit(k, Some(pc), pending, j, 0);
        self.extra -= 1;
        self.i(I::End);
        self.perm_check(w);
        // fast: P == Q && P == 2
        self.i(I::LocalGet(P));
        self.i(I::LocalGet(Q));
        self.i(I::I32Eq);
        self.i(I::LocalGet(P));
        self.i(I::I32Const(2));
        self.i(I::I32Eq);
        self.i(I::I32And);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.guest_offset();
        self.get(rs2);
        self.i(match kind {
            StoreKind::B => I::I32Store8(memarg(0)),
            StoreKind::H => I::I32Store16(memarg(0)),
            StoreKind::W => I::I32Store(memarg(0)),
        });
        self.i(I::Else);
        self.i(I::LocalGet(P));
        self.i(I::LocalGet(Q));
        self.i(I::I32Or);
        self.i(I::I32Eqz);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.i(I::I32Const(pc as i32));
        self.i(I::LocalGet(CYC));
        self.i(I::I64Const(pending as i64));
        self.i(I::I64Add);
        self.i(I::LocalGet(A));
        self.i(I::I32Const(kind_code));
        self.get(rs2);
        self.i(I::Call(F_MMIO_STORE));
        self.i(I::LocalSet(ST));
        self.i(I::LocalGet(ST));
        self.i(I::I32Const(1));
        self.i(I::I32Eq);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.exit(k, Some(pc), pending, j, 0);
        self.extra -= 1;
        self.i(I::End);
        self.i(I::Else);
        self.exit(k, Some(pc), pending, j, 0);
        self.i(I::End);
        self.extra -= 1;
        self.i(I::End);
        self.extra -= 1;
        self.i(I::LocalGet(ST));
        self.i(I::I32Const(2));
        self.i(I::I32Eq);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.exit(k, Some(pc.wrapping_add(u32::from(width))), pending + cost, j + 1, 1);
        self.extra -= 1;
        self.i(I::End);
    }

    fn op(&mut self, op: Op, rd: u8, rs1: u8, rs2: u8) {
        match op {
            Op::Mulh | Op::Mulhsu | Op::Mulhu => {
                self.get(rs1);
                self.i(if op == Op::Mulhu {
                    I::I64ExtendI32U
                } else {
                    I::I64ExtendI32S
                });
                self.get(rs2);
                self.i(if op == Op::Mulh {
                    I::I64ExtendI32S
                } else {
                    I::I64ExtendI32U
                });
                self.i(I::I64Mul);
                self.i(I::I64Const(32));
                self.i(if op == Op::Mulhu {
                    I::I64ShrU
                } else {
                    I::I64ShrS
                });
                self.i(I::I32WrapI64);
            }
            Op::Div | Op::Divu | Op::Rem | Op::Remu => {
                self.get(rs2);
                self.i(I::LocalSet(T));
                self.i(I::LocalGet(T));
                self.i(I::I32Eqz);
                self.i(I::If(BlockType::Result(ValType::I32)));
                match op {
                    Op::Div | Op::Divu => self.i(I::I32Const(-1)),
                    _ => self.get(rs1),
                }
                self.i(I::Else);
                match op {
                    Op::Div | Op::Rem => {
                        self.i(I::LocalGet(T));
                        self.i(I::I32Const(-1));
                        self.i(I::I32Eq);
                        self.i(I::If(BlockType::Result(ValType::I32)));
                        if op == Op::Div {
                            self.i(I::I32Const(0));
                            self.get(rs1);
                            self.i(I::I32Sub);
                        } else {
                            self.i(I::I32Const(0));
                        }
                        self.i(I::Else);
                        self.get(rs1);
                        self.i(I::LocalGet(T));
                        self.i(if op == Op::Div { I::I32DivS } else { I::I32RemS });
                        self.i(I::End);
                    }
                    _ => {
                        self.get(rs1);
                        self.i(I::LocalGet(T));
                        self.i(if op == Op::Divu {
                            I::I32DivU
                        } else {
                            I::I32RemU
                        });
                    }
                }
                self.i(I::End);
            }
            _ => {
                self.get(rs1);
                self.get(rs2);
                self.i(match op {
                    Op::Add => I::I32Add,
                    Op::Sub => I::I32Sub,
                    Op::Sll => I::I32Shl,
                    Op::Slt => I::I32LtS,
                    Op::Sltu => I::I32LtU,
                    Op::Xor => I::I32Xor,
                    Op::Srl => I::I32ShrU,
                    Op::Sra => I::I32ShrS,
                    Op::Or => I::I32Or,
                    Op::And => I::I32And,
                    Op::Mul => I::I32Mul,
                    _ => unreachable!(),
                });
            }
        }
        self.set(rd);
    }

    fn op_imm(&mut self, op: OpI, rd: u8, rs1: u8, imm: i32) {
        self.get(rs1);
        self.i(I::I32Const(imm));
        self.i(match op {
            OpI::Addi => I::I32Add,
            OpI::Slti => I::I32LtS,
            OpI::Sltiu => I::I32LtU,
            OpI::Xori => I::I32Xor,
            OpI::Ori => I::I32Or,
            OpI::Andi => I::I32And,
            OpI::Slli => I::I32Shl,
            OpI::Srli => I::I32ShrU,
            OpI::Srai => I::I32ShrS,
        });
        self.set(rd);
    }

    fn block(&mut self, k: usize) {
        let code = self.code;
        let b = &code.blocks[k];
        let len = b.insts.len() as u32;
        // Budget: not one instruction may start at or past `end`, so the
        // block runs only if all of it fits under the most it can cost.
        let max: u64 = b.insts.iter().map(|(_, d)| self.cost(d.class)).sum();
        self.i(I::LocalGet(CYC));
        self.i(I::I64Const(max as i64));
        self.i(I::I64Add);
        self.i(I::LocalGet(P_END));
        self.i(I::I64GtU);
        self.i(I::If(BlockType::Empty));
        self.extra += 1;
        self.exit(k, Some(b.pc), 0, 0, 0);
        self.extra -= 1;
        self.i(I::End);

        let mut pending = 0u64;
        for (j, (pc, d)) in b.insts.iter().enumerate() {
            let pc = *pc;
            let c = self.cost(d.class);
            let j32 = j as u32;
            match d.inst {
                Inst::Lui { rd, imm } => {
                    self.i(I::I32Const(imm));
                    self.set(rd);
                }
                Inst::Auipc { rd, imm } => {
                    self.i(I::I32Const(pc.wrapping_add(imm as u32) as i32));
                    self.set(rd);
                }
                Inst::OpImm { op, rd, rs1, imm } => self.op_imm(op, rd, rs1, imm),
                Inst::Op { op, rd, rs1, rs2 } => self.op(op, rd, rs1, rs2),
                Inst::Fence | Inst::Nop => {}
                Inst::Load { kind, rd, rs1, imm } => {
                    self.load(k, pc, d.width, c, pending, j32, kind, rd, rs1, imm);
                }
                Inst::Store { kind, rs1, rs2, imm } => {
                    self.store(k, pc, d.width, c, pending, j32, kind, rs1, rs2, imm);
                }
                Inst::Branch { cond, rs1, rs2, imm } => {
                    let taken = self.cost(InstClass::BranchTaken);
                    let not_taken = self.cost(InstClass::BranchNotTaken);
                    self.get(rs1);
                    self.get(rs2);
                    self.i(match cond {
                        Cond::Eq => I::I32Eq,
                        Cond::Ne => I::I32Ne,
                        Cond::Lt => I::I32LtS,
                        Cond::Ge => I::I32GeS,
                        Cond::Ltu => I::I32LtU,
                        Cond::Geu => I::I32GeU,
                    });
                    self.i(I::If(BlockType::Empty));
                    self.extra += 1;
                    self.add_cyc(pending + taken);
                    self.add_ran(len);
                    self.goto(k, pc.wrapping_add(imm as u32));
                    self.extra -= 1;
                    self.i(I::End);
                    self.add_cyc(pending + not_taken);
                    self.add_ran(len);
                    self.fall_through(k, pc.wrapping_add(u32::from(d.width)));
                    return;
                }
                Inst::Jal { rd, imm } => {
                    if rd != 0 {
                        self.i(I::I32Const(pc.wrapping_add(u32::from(d.width)) as i32));
                        self.set(rd);
                    }
                    self.add_cyc(pending + c);
                    self.add_ran(len);
                    self.goto(k, pc.wrapping_add(imm as u32) & !1);
                    return;
                }
                Inst::Jalr { rd, rs1, imm } => {
                    self.get(rs1);
                    self.i(I::I32Const(imm));
                    self.i(I::I32Add);
                    self.i(I::I32Const(-2));
                    self.i(I::I32And);
                    self.i(I::LocalSet(A));
                    if rd != 0 {
                        self.i(I::I32Const(pc.wrapping_add(u32::from(d.width)) as i32));
                        self.set(rd);
                    }
                    self.add_cyc(pending + c);
                    self.add_ran(len);
                    for t in code.targets.clone() {
                        if !code.index.contains_key(&t) {
                            continue;
                        }
                        self.i(I::LocalGet(A));
                        self.i(I::I32Const(t as i32));
                        self.i(I::I32Eq);
                        self.i(I::If(BlockType::Empty));
                        self.extra += 1;
                        self.goto(k, t);
                        self.extra -= 1;
                        self.i(I::End);
                    }
                    self.exit(k, None, 0, 0, 0);
                    return;
                }
            }
            pending += c;
        }
        // No terminator: fell off the block.
        self.add_cyc(pending);
        self.add_ran(len);
        match b.end {
            BlockEnd::Term => unreachable!(),
            BlockEnd::Fall(pc) => self.fall_through(k, pc),
            BlockEnd::Undecodable(pc) => self.exit(k, Some(pc), 0, 0, 0),
        }
    }

    /// Continue at `pc`, which is free when it is the next block in layout.
    fn fall_through(&mut self, k: usize, pc: u32) {
        if k + 1 < self.n + 1 && self.code.blocks.get(k + 1).is_some_and(|b| b.pc == pc) {
            return;
        }
        self.goto(k, pc);
    }
}

/// Registers the region reads or writes anywhere, so the prologue loads and
/// the epilogue stores exactly those.
fn live_regs(code: &RegionCode) -> Vec<u8> {
    let mut live = [false; 32];
    let mut mark = |r: u8| live[r as usize] = true;
    for b in &code.blocks {
        for (_, d) in &b.insts {
            match d.inst {
                Inst::Lui { rd, .. } | Inst::Auipc { rd, .. } | Inst::Jal { rd, .. } => mark(rd),
                Inst::Jalr { rd, rs1, .. } => {
                    mark(rd);
                    mark(rs1);
                }
                Inst::Branch { rs1, rs2, .. } => {
                    mark(rs1);
                    mark(rs2);
                }
                Inst::Load { rd, rs1, .. } => {
                    mark(rd);
                    mark(rs1);
                }
                Inst::Store { rs1, rs2, .. } => {
                    mark(rs1);
                    mark(rs2);
                }
                Inst::OpImm { rd, rs1, .. } => {
                    mark(rd);
                    mark(rs1);
                }
                Inst::Op { rd, rs1, rs2, .. } => {
                    mark(rd);
                    mark(rs1);
                    mark(rs2);
                }
                Inst::Fence | Inst::Nop => {}
            }
        }
    }
    (1u8..32).filter(|&r| live[r as usize]).collect()
}

/// Emit the module for `code` under `model`. Returns the wasm bytes.
pub fn emit(code: &RegionCode, model: CycleModel) -> Vec<u8> {
    let n = code.blocks.len() - 1;
    let mut module = Module::new();

    let mut types = TypeSection::new();
    types
        .ty()
        .function([ValType::I32, ValType::I64, ValType::I64], [ValType::I32]);
    types.ty().function(
        [ValType::I32, ValType::I64, ValType::I32, ValType::I32],
        [ValType::I64],
    );
    types.ty().function(
        [
            ValType::I32,
            ValType::I64,
            ValType::I32,
            ValType::I32,
            ValType::I32,
        ],
        [ValType::I32],
    );
    module.section(&types);

    let mut imports = ImportSection::new();
    imports.import(
        "env",
        "mmio_load",
        EntityType::Function(1),
    );
    imports.import(
        "env",
        "mmio_store",
        EntityType::Function(2),
    );
    imports.import(
        "env",
        "mem",
        EntityType::Memory(MemoryType {
            minimum: PAGES,
            maximum: Some(PAGES),
            memory64: false,
            shared: false,
            page_size_log2: None,
        }),
    );
    module.section(&imports);

    let mut funcs = FunctionSection::new();
    funcs.function(0);
    module.section(&funcs);

    let mut exports = ExportSection::new();
    exports.export("run", ExportKind::Func, 2);
    module.section(&exports);

    let locals = vec![
        (31, ValType::I32), // x1..x31
        (1, ValType::I64),  // CYC
        (5, ValType::I32),  // RAN EXIT_PC FLAG NEXT A
        (3, ValType::I32),  // P Q ST
        (1, ValType::I64),  // T64
        (1, ValType::I32),  // T
        (2, ValType::I64),  // WLO WHI
    ];
    let mut e = Emitter {
        f: Function::new(locals),
        code,
        model,
        n,
        extra: 0,
    };
    let live = live_regs(code);

    e.i(I::LocalGet(P_CYC0));
    e.i(I::LocalSet(CYC));
    for &r in &live {
        e.i(I::I32Const(0));
        e.i(I::I32Load(memarg(u64::from(SCRATCH) + 4 * u64::from(r))));
        e.i(I::LocalSet(reg_local(r)));
    }
    e.i(I::I32Const(0));
    e.i(I::I64Load(memarg(u64::from(SCRATCH) + 144)));
    e.i(I::LocalSet(WLO));
    e.i(I::I32Const(0));
    e.i(I::I64Load(memarg(u64::from(SCRATCH) + 152)));
    e.i(I::LocalSet(WHI));
    e.i(I::LocalGet(P_ENTRY));
    e.i(I::LocalSet(NEXT));

    e.i(I::Block(BlockType::Empty)); // exit
    e.i(I::Loop(BlockType::Empty)); // dispatch
    for _ in 0..=n {
        e.i(I::Block(BlockType::Empty)); // b_n .. b_0
    }
    e.i(I::Block(BlockType::Empty)); // bad
    e.i(I::LocalGet(NEXT));
    let table: Vec<u32> = (1..=(n as u32 + 1)).collect();
    e.i(I::BrTable(Cow::Owned(table), 0));
    e.i(I::End); // bad
    e.i(I::Unreachable);
    for k in 0..=n {
        e.i(I::End); // b_k
        e.block(k);
    }
    e.i(I::End); // dispatch
    e.i(I::End); // exit

    for &r in &live {
        e.i(I::I32Const(0));
        e.i(I::LocalGet(reg_local(r)));
        e.i(I::I32Store(memarg(u64::from(SCRATCH) + 4 * u64::from(r))));
    }
    e.i(I::I32Const(0));
    e.i(I::LocalGet(CYC));
    e.i(I::I64Store(memarg(u64::from(SCRATCH) + 128)));
    e.i(I::I32Const(0));
    e.i(I::LocalGet(RAN));
    e.i(I::I32Store(memarg(u64::from(SCRATCH) + 136)));
    e.i(I::I32Const(0));
    e.i(I::LocalGet(FLAG));
    e.i(I::I32Store(memarg(u64::from(SCRATCH) + 140)));
    e.i(I::LocalGet(EXIT_PC));
    e.i(I::End);

    let mut codes = CodeSection::new();
    codes.function(&e.f);
    module.section(&codes);
    module.finish()
}
