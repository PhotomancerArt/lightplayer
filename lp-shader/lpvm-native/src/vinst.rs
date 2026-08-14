//! Virtual instructions: post-lowering, pre-regalloc.
//!
//! VInsts model RISC-V instruction formats (R-type, I-type, etc.) rather than
//! LPIR semantics. This keeps the operand shapes aligned with the target ISA and
//! makes immediate folding, native floats, and inline Q32 natural extensions.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// LPIR virtual register at the IR boundary; lowered to [`VReg`] (`u16`).
pub type IrVReg = lpir::VReg;

/// Virtual register index after lowering. Ids are uncapped within `u16`:
/// large functions (and [`TempVRegs`] temps) routinely mint past
/// [`crate::config::MAX_VREGS`], which is only [`crate::regset::RegSet`]'s
/// inline-storage threshold, not a limit.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Ord, PartialOrd)]
pub struct VReg(pub u16);

/// Half-open slice into a per-function [`Vec<VReg>`](alloc::vec::Vec) (Call args / Ret values).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VRegSlice {
    pub start: u16,
    pub count: u8,
}

impl VRegSlice {
    #[must_use]
    pub fn vregs<'a>(&self, pool: &'a [VReg]) -> &'a [VReg] {
        let s = self.start as usize;
        let e = s + self.count as usize;
        &pool[s..e]
    }

    #[must_use]
    pub fn len(self) -> usize {
        self.count as usize
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.count == 0
    }
}

/// Index into [`ModuleSymbols::names`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SymbolId(pub u16);

/// Module-level intern table for callee names (built during lowering).
#[derive(Default, Debug, Clone)]
pub struct ModuleSymbols {
    pub names: Vec<String>,
}

impl ModuleSymbols {
    /// Intern a symbol name. Takes `&str` so the common hit path (symbols
    /// repeat heavily — every Q32 float op calls the same few builtins)
    /// performs no allocation; only a first-time insert copies the name.
    pub fn intern(&mut self, name: &str) -> SymbolId {
        if let Some(i) = self.names.iter().position(|n| n == name) {
            return SymbolId(i as u16);
        }
        let id = self.names.len();
        assert!(
            id < usize::from(u16::MAX),
            "ModuleSymbols::intern: too many symbols"
        );
        self.names.push(String::from(name));
        SymbolId(id as u16)
    }

    #[must_use]
    pub fn name(&self, id: SymbolId) -> &str {
        &self.names[id.0 as usize]
    }
}

/// Sentinel: no originating LPIR op index ([`VInst::src_op`]).
pub const SRC_OP_NONE: u16 = 0xFFFF;

#[inline]
pub const fn pack_src_op(src_op: Option<u32>) -> u16 {
    match src_op {
        None => SRC_OP_NONE,
        Some(i) if i <= u16::MAX as u32 => i as u16,
        Some(_) => SRC_OP_NONE,
    }
}

#[inline]
pub const fn unpack_src_op(src_op: u16) -> Option<u32> {
    if src_op == SRC_OP_NONE {
        None
    } else {
        Some(src_op as u32)
    }
}

/// Label id for branch targets.
pub type LabelId = u32;

// ─── ALU opcode enums ────────────────────────────────────────────────────────

/// R-type ALU operations (register-register: `rd = rs1 OP rs2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum AluOp {
    Add,
    Sub,
    Mul,
    /// High half of signed `rs1 * rs2` (RISC-V `mulh`).
    MulH,
    And,
    Or,
    Xor,
    Sll,
    SrlU,
    SraS,
    DivS,
    DivU,
    RemS,
    RemU,
}

impl AluOp {
    pub fn mnemonic(self) -> &'static str {
        match self {
            AluOp::Add => "Add",
            AluOp::Sub => "Sub",
            AluOp::Mul => "Mul",
            AluOp::MulH => "MulH",
            AluOp::And => "And",
            AluOp::Or => "Or",
            AluOp::Xor => "Xor",
            AluOp::Sll => "Sll",
            AluOp::SrlU => "SrlU",
            AluOp::SraS => "SraS",
            AluOp::DivS => "DivS",
            AluOp::DivU => "DivU",
            AluOp::RemS => "RemS",
            AluOp::RemU => "RemU",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            AluOp::Add => "+",
            AluOp::Sub => "-",
            AluOp::Mul => "*",
            AluOp::MulH => "*h",
            AluOp::And => "&",
            AluOp::Or => "|",
            AluOp::Xor => "^",
            AluOp::Sll => "<<",
            AluOp::SrlU => ">>u",
            AluOp::SraS => ">>",
            AluOp::DivS => "/",
            AluOp::DivU => "/u",
            AluOp::RemS => "%",
            AluOp::RemU => "%u",
        }
    }

    pub fn from_mnemonic(s: &str) -> Option<Self> {
        match s {
            "Add" => Some(AluOp::Add),
            "Sub" => Some(AluOp::Sub),
            "Mul" => Some(AluOp::Mul),
            "MulH" => Some(AluOp::MulH),
            "And" => Some(AluOp::And),
            "Or" => Some(AluOp::Or),
            "Xor" => Some(AluOp::Xor),
            "Sll" => Some(AluOp::Sll),
            "SrlU" => Some(AluOp::SrlU),
            "SraS" => Some(AluOp::SraS),
            "DivS" => Some(AluOp::DivS),
            "DivU" => Some(AluOp::DivU),
            "RemS" => Some(AluOp::RemS),
            "RemU" => Some(AluOp::RemU),
            _ => None,
        }
    }
}

/// I-type ALU operations (register-immediate: `rd = rs1 OP imm12`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum AluImmOp {
    Addi,
    Andi,
    Ori,
    Xori,
    Slli,
    SrliU,
    SraiS,
    Slti,
    SltiU,
}

impl AluImmOp {
    pub fn mnemonic(self) -> &'static str {
        match self {
            AluImmOp::Addi => "Addi",
            AluImmOp::Andi => "Andi",
            AluImmOp::Ori => "Ori",
            AluImmOp::Xori => "Xori",
            AluImmOp::Slli => "Slli",
            AluImmOp::SrliU => "SrliU",
            AluImmOp::SraiS => "SraiS",
            AluImmOp::Slti => "Slti",
            AluImmOp::SltiU => "SltiU",
        }
    }

    pub fn from_mnemonic(s: &str) -> Option<Self> {
        match s {
            "Addi" => Some(AluImmOp::Addi),
            "Andi" => Some(AluImmOp::Andi),
            "Ori" => Some(AluImmOp::Ori),
            "Xori" => Some(AluImmOp::Xori),
            "Slli" => Some(AluImmOp::Slli),
            "SrliU" => Some(AluImmOp::SrliU),
            "SraiS" => Some(AluImmOp::SraiS),
            "Slti" => Some(AluImmOp::Slti),
            "SltiU" => Some(AluImmOp::SltiU),
            _ => None,
        }
    }
}

// ─── Float opcode enums ──────────────────────────────────────────────────────
//
// These name IEEE-754 binary32 operations, not one ISA's mnemonics. Each maps
// to a single hardware instruction on a target with an FPU; on a soft-float
// target the ops never appear at all, because that lowering keeps every value
// integer-class and emits ordinary `Call`s (see [`crate::lower_f32`]).

/// Three-operand float arithmetic: `dst = src1 OP src2`.
///
/// Only the operations that are **one instruction everywhere** are here.
/// Division is not: it is a builtin call on both float targets (M7 D4), so
/// giving it a VInst would create a variant no backend could emit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FAluOp {
    Add,
    Sub,
    Mul,
}

impl FAluOp {
    pub fn mnemonic(self) -> &'static str {
        match self {
            FAluOp::Add => "FAdd",
            FAluOp::Sub => "FSub",
            FAluOp::Mul => "FMul",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            FAluOp::Add => "+.f",
            FAluOp::Sub => "-.f",
            FAluOp::Mul => "*.f",
        }
    }

    pub fn from_mnemonic(s: &str) -> Option<Self> {
        match s {
            "FAdd" => Some(FAluOp::Add),
            "FSub" => Some(FAluOp::Sub),
            "FMul" => Some(FAluOp::Mul),
            _ => None,
        }
    }
}

/// Two-operand float ops: `dst = OP src`.
///
/// `Abs` and `Neg` are sign-bit operations, **not** `0 - x` and `x < 0 ? -x : x`
/// — they must stay exact on NaN and `-0.0` (`docs/design/float.md` §3). The
/// hardware `abs.s`/`neg.s` are defined that way; the soft-float path spells
/// the same thing as an integer mask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FAluRROp {
    Mov,
    Abs,
    Neg,
}

impl FAluRROp {
    pub fn mnemonic(self) -> &'static str {
        match self {
            FAluRROp::Mov => "FMov",
            FAluRROp::Abs => "FAbs",
            FAluRROp::Neg => "FNeg",
        }
    }

    pub fn from_mnemonic(s: &str) -> Option<Self> {
        match s {
            "FMov" => Some(FAluRROp::Mov),
            "FAbs" => Some(FAluRROp::Abs),
            "FNeg" => Some(FAluRROp::Neg),
            _ => None,
        }
    }
}

/// IEEE-754 comparison predicate for [`VInst::Fcmp`].
///
/// The NaN behavior is normative, not incidental (`docs/design/float.md` §3):
/// every **ordered** comparison is false when either operand is NaN, and `Ne`
/// is true. That is why [`FcmpCond::Ne`] is its own condition rather than a
/// negated [`FcmpCond::Eq`] — the two differ exactly on NaN, which is the one
/// input where the rewrite would be observable and wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FcmpCond {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl FcmpCond {
    pub fn mnemonic(self) -> &'static str {
        match self {
            FcmpCond::Eq => "Eq",
            FcmpCond::Ne => "Ne",
            FcmpCond::Lt => "Lt",
            FcmpCond::Le => "Le",
            FcmpCond::Gt => "Gt",
            FcmpCond::Ge => "Ge",
        }
    }

    pub fn from_mnemonic(s: &str) -> Option<Self> {
        match s {
            "Eq" => Some(FcmpCond::Eq),
            "Ne" => Some(FcmpCond::Ne),
            "Lt" => Some(FcmpCond::Lt),
            "Le" => Some(FcmpCond::Le),
            "Gt" => Some(FcmpCond::Gt),
            "Ge" => Some(FcmpCond::Ge),
            _ => None,
        }
    }
}

fn fcmp_cond_op(cond: FcmpCond) -> &'static str {
    match cond {
        FcmpCond::Eq => "==.f",
        FcmpCond::Ne => "!=.f",
        FcmpCond::Lt => "<.f",
        FcmpCond::Le => "<=.f",
        FcmpCond::Gt => ">.f",
        FcmpCond::Ge => ">=.f",
    }
}

// ─── Comparison condition ────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IcmpCond {
    Eq,
    Ne,
    LtS,
    LeS,
    GtS,
    GeS,
    LtU,
    LeU,
    GtU,
    GeU,
}

fn icmp_cond_op(cond: IcmpCond) -> &'static str {
    match cond {
        IcmpCond::Eq => "==",
        IcmpCond::Ne => "!=",
        IcmpCond::LtS => "<",
        IcmpCond::LeS => "<=",
        IcmpCond::GtS => ">",
        IcmpCond::GeS => ">=",
        IcmpCond::LtU => "<u",
        IcmpCond::LeU => "<=u",
        IcmpCond::GtU => ">u",
        IcmpCond::GeU => ">=u",
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn vregs_csv_pool(pool: &[VReg], slice: VRegSlice) -> String {
    slice
        .vregs(pool)
        .iter()
        .map(|r| format!("v{}", r.0))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Watermark allocator for fresh temporary [`VReg`]s during lowering.
///
/// Initialized to `func.vreg_types.len() as u16` (i.e. one past the
/// highest IR-declared vreg). Each [`Self::mint`] call returns a fresh
/// [`VReg`] above the IR vreg space; ids never collide with IR vregs and
/// never reset across LPIR ops within a function.
///
/// Used by [`crate::lower::lower_lpir_op`] when an op expands to
/// multiple [`VInst`]s and needs intermediate registers.
#[derive(Clone, Copy, Debug)]
pub struct TempVRegs(u16);

impl TempVRegs {
    pub fn new(after_ir: u16) -> Self {
        Self(after_ir)
    }

    pub fn mint(&mut self) -> VReg {
        let v = VReg(self.0);
        self.0 = self
            .0
            .checked_add(1)
            .expect("lpvm-native: temp vreg space exhausted (u16)");
        v
    }
}

// ─── VInst ───────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VInst {
    /// R-type: `dst = src1 OP src2` (add, sub, mul, and, or, xor, shifts, div, rem).
    AluRRR {
        op: AluOp,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: u16,
    },
    /// I-type: `dst = src OP imm12` (addi, andi, ori, xori, slli, srli, srai, slti, sltiu).
    AluRRI {
        op: AluImmOp,
        dst: VReg,
        src: VReg,
        imm: i32,
        src_op: u16,
    },
    /// Unary negate: `dst = -src` (pseudo for `sub rd, x0, rs`).
    Neg { dst: VReg, src: VReg, src_op: u16 },
    /// Bitwise NOT: `dst = ~src` (pseudo for `xori rd, rs, -1`).
    Bnot { dst: VReg, src: VReg, src_op: u16 },
    /// Integer comparison (pseudo — multi-instruction expansion in emitter).
    Icmp {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        cond: IcmpCond,
        src_op: u16,
    },
    /// Integer comparison with immediate (pseudo).
    IcmpImm {
        dst: VReg,
        src: VReg,
        imm: i32,
        cond: IcmpCond,
        src_op: u16,
    },
    /// Select: `dst = cond ? if_true : if_false`.
    Select {
        dst: VReg,
        cond: VReg,
        if_true: VReg,
        if_false: VReg,
        src_op: u16,
    },
    /// Unconditional branch.
    Br { target: LabelId, src_op: u16 },
    /// Conditional branch.
    BrIf {
        cond: VReg,
        target: LabelId,
        invert: bool,
        src_op: u16,
    },
    /// Register copy (kept separate for copy-coalescing in regalloc).
    Mov { dst: VReg, src: VReg, src_op: u16 },
    /// Word load: `dst = [base + offset]`.
    Load32 {
        dst: VReg,
        base: VReg,
        offset: i32,
        src_op: u16,
    },
    /// Word store: `[base + offset] = src`.
    Store32 {
        src: VReg,
        base: VReg,
        offset: i32,
        src_op: u16,
    },
    /// 8-bit store: `[base + offset] = src` (low 8 bits).
    Store8 {
        src: VReg,
        base: VReg,
        offset: i32,
        src_op: u16,
    },
    /// 16-bit store: `[base + offset] = src` (low 16 bits).
    Store16 {
        src: VReg,
        base: VReg,
        offset: i32,
        src_op: u16,
    },
    /// Zero-extending byte load: `dst = u8[base + offset]`.
    Load8U {
        dst: VReg,
        base: VReg,
        offset: i32,
        src_op: u16,
    },
    /// Sign-extending byte load: `dst = i8[base + offset]`.
    Load8S {
        dst: VReg,
        base: VReg,
        offset: i32,
        src_op: u16,
    },
    /// Zero-extending halfword load.
    Load16U {
        dst: VReg,
        base: VReg,
        offset: i32,
        src_op: u16,
    },
    /// Sign-extending halfword load.
    Load16S {
        dst: VReg,
        base: VReg,
        offset: i32,
        src_op: u16,
    },
    /// Compute address of LPIR stack slot.
    SlotAddr { dst: VReg, slot: u32, src_op: u16 },
    /// Word-aligned memcpy.
    MemcpyWords {
        dst_base: VReg,
        src_base: VReg,
        size: u32,
        src_op: u16,
    },
    /// 32-bit integer constant load.
    IConst32 { dst: VReg, val: i32, src_op: u16 },
    /// Function call.
    Call {
        target: SymbolId,
        args: VRegSlice,
        rets: VRegSlice,
        callee_uses_sret: bool,
        /// When set with [`Self::Call::callee_uses_sret`], LPIR `Call.args` already includes
        /// the callee's hidden sret pointer (`[vmctx, sret, …]`). When clear, the emitter synthesizes
        /// `a0` from the caller stack slot (legacy many-scalar-return path).
        caller_passes_sret_ptr: bool,
        /// When `caller_passes_sret_ptr` is set: if true, RV32 assigns the first two args like
        /// shader calls (`vmctx → a1`, `sret → a0`). If false (`@texture::*`-style `[sret, …]` with
        /// no [`ImportDecl::needs_vmctx`]), arguments map sequentially from `a0`.
        caller_sret_vm_abi_swap: bool,
        src_op: u16,
    },
    /// Return from function.
    Ret { vals: VRegSlice, src_op: u16 },
    /// Label definition (branch target).
    Label(LabelId, u16),
    /// Fuel check (pseudo — multi-instruction expansion in the emitter).
    ///
    /// Loads the fuel counter (vmctx low fuel word); if it is zero, writes
    /// [`lpvm::TRAP_CODE_OUT_OF_FUEL`] to the vmctx trap slot and jumps to
    /// `trap_label` (the function's epilogue). Otherwise, when `decrement`
    /// is set, subtracts 1 and stores the counter back (check-then-decrement:
    /// the trap fires when a check *observes* 0, not when the counter reaches
    /// 0). Inserted at function entry (`decrement: false`) and at every loop
    /// back-edge (`decrement: true`) when fuel metering is enabled.
    FuelCheck {
        /// The vmctx pointer vreg (a plain use; regalloc handles spill/reload).
        vmctx: VReg,
        decrement: bool,
        /// Branch target on exhaustion: the function's epilogue label.
        trap_label: LabelId,
        src_op: u16,
    },

    // ─── Hardware float ──────────────────────────────────────────────────────
    //
    // Present unconditionally, not behind `float-f32` (M7 D9 / Q5). Feature-
    // gating enum variants that five per-variant helpers and a text ser/de
    // module all match exhaustively costs far more than the residual it saves,
    // and the residual is measured rather than argued about. Nothing constructs
    // these unless the f32 lowering module is linked.
    //
    // **Measured residual, ESP32-C6 image with `float-f32` off** (M7 P1/P2,
    // against `2862448 B`): `.text` −120 B, `.rodata` +1,508 B, `.data` +32 B,
    // image +1,424 B. The cost is *not* code — it is the jump tables of the
    // per-variant matches (`for_each_def`, `for_each_use`, `src_op`, the
    // emitter's dispatch) growing by nine entries at every site they inline
    // into. Everything that could be gated has been: the class map, the float
    // register tables, the lowering module, and the emitter's diagnostic
    // strings. What is left is the price of D9, in the form D9 asked to see it.
    //
    // The operand *class* of every field below is fixed and is what
    // `regalloc::classes` must report. Getting one wrong does not crash: it
    // silently reinterprets a bit pattern between the two register files, which
    // is why `regalloc::verify::verify_operand_classes` exists.
    /// `dst = src1 OP src2`, one FP instruction. **dst/src1/src2 all Float.**
    FAluRRR {
        op: FAluOp,
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: u16,
    },
    /// `dst = OP src`, one FP instruction. **dst/src Float.**
    FAluRR {
        op: FAluRROp,
        dst: VReg,
        src: VReg,
        src_op: u16,
    },
    /// IEEE comparison producing an integer 0/1. **dst Int; lhs/rhs Float.**
    ///
    /// The mixed classes are the point. On Xtensa the comparison itself writes
    /// a Boolean register, and the emitter materializes 0/1 into an address
    /// register within the same sequence (M7 D5), so no Boolean register is
    /// ever live at a `VInst` boundary and the allocator never sees a third
    /// register class.
    Fcmp {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        cond: FcmpCond,
        src_op: u16,
    },
    /// `dst = cond != 0 ? if_true : if_false`.
    /// **dst/if_true/if_false Float; cond Int.**
    FSelect {
        dst: VReg,
        cond: VReg,
        if_true: VReg,
        if_false: VReg,
        src_op: u16,
    },
    /// Word load into the float file: `dst = [base + offset]`.
    /// **dst Float, base Int.**
    FLoad32 {
        dst: VReg,
        base: VReg,
        offset: i32,
        src_op: u16,
    },
    /// Word store from the float file: `[base + offset] = src`.
    /// **src Float, base Int.**
    FStore32 {
        src: VReg,
        base: VReg,
        offset: i32,
        src_op: u16,
    },
    /// Integer register → float register, **bit-for-bit**. **dst Float, src Int.**
    ///
    /// Not a conversion — [`VInst::IToF`] is the conversion. This is the
    /// boundary transfer that implements "float values travel in address
    /// registers as raw IEEE bit patterns" (M7 D1/D2), and lowering is the only
    /// thing that emits it.
    Wfr { dst: VReg, src: VReg, src_op: u16 },
    /// Float register → integer register, bit-for-bit. **dst Int, src Float.**
    ///
    /// The inverse of [`VInst::Wfr`]; see its note.
    Rfr { dst: VReg, src: VReg, src_op: u16 },
    /// Integer → float **conversion**, correctly rounded. **dst Float, src Int.**
    ///
    /// `signed` selects the interpretation of `src`. Inlined where
    /// division and the float→int direction are not (M7 D4) because both
    /// signednesses are a single correctly-rounded instruction with no
    /// saturation question attached.
    IToF {
        dst: VReg,
        src: VReg,
        signed: bool,
        src_op: u16,
    },
}

impl VInst {
    /// Index of the originating LPIR op in [`lpir::IrFunction::body`], when tracked.
    pub fn src_op(&self) -> Option<u32> {
        let raw = match self {
            VInst::AluRRR { src_op, .. }
            | VInst::AluRRI { src_op, .. }
            | VInst::Neg { src_op, .. }
            | VInst::Bnot { src_op, .. }
            | VInst::Icmp { src_op, .. }
            | VInst::IcmpImm { src_op, .. }
            | VInst::Select { src_op, .. }
            | VInst::Br { src_op, .. }
            | VInst::BrIf { src_op, .. }
            | VInst::Mov { src_op, .. }
            | VInst::Load32 { src_op, .. }
            | VInst::Store32 { src_op, .. }
            | VInst::Store8 { src_op, .. }
            | VInst::Store16 { src_op, .. }
            | VInst::Load8U { src_op, .. }
            | VInst::Load8S { src_op, .. }
            | VInst::Load16U { src_op, .. }
            | VInst::Load16S { src_op, .. }
            | VInst::SlotAddr { src_op, .. }
            | VInst::MemcpyWords { src_op, .. }
            | VInst::IConst32 { src_op, .. }
            | VInst::Call { src_op, .. }
            | VInst::Ret { src_op, .. }
            | VInst::FuelCheck { src_op, .. }
            | VInst::FAluRRR { src_op, .. }
            | VInst::FAluRR { src_op, .. }
            | VInst::Fcmp { src_op, .. }
            | VInst::FSelect { src_op, .. }
            | VInst::FLoad32 { src_op, .. }
            | VInst::FStore32 { src_op, .. }
            | VInst::Wfr { src_op, .. }
            | VInst::Rfr { src_op, .. }
            | VInst::IToF { src_op, .. } => *src_op,
            VInst::Label(_, src_op) => *src_op,
        };
        unpack_src_op(raw)
    }

    pub fn for_each_def<F: FnMut(VReg)>(&self, pool: &[VReg], mut f: F) {
        match self {
            VInst::AluRRR { dst, .. }
            | VInst::AluRRI { dst, .. }
            | VInst::Neg { dst, .. }
            | VInst::Bnot { dst, .. }
            | VInst::Icmp { dst, .. }
            | VInst::IcmpImm { dst, .. }
            | VInst::Select { dst, .. }
            | VInst::Mov { dst, .. }
            | VInst::Load32 { dst, .. }
            | VInst::Load8U { dst, .. }
            | VInst::Load8S { dst, .. }
            | VInst::Load16U { dst, .. }
            | VInst::Load16S { dst, .. }
            | VInst::SlotAddr { dst, .. }
            | VInst::IConst32 { dst, .. }
            | VInst::FAluRRR { dst, .. }
            | VInst::FAluRR { dst, .. }
            | VInst::Fcmp { dst, .. }
            | VInst::FSelect { dst, .. }
            | VInst::FLoad32 { dst, .. }
            | VInst::Wfr { dst, .. }
            | VInst::Rfr { dst, .. }
            | VInst::IToF { dst, .. } => f(*dst),
            VInst::Store32 { .. }
            | VInst::Store8 { .. }
            | VInst::Store16 { .. }
            | VInst::MemcpyWords { .. }
            | VInst::Label(..)
            | VInst::Br { .. }
            | VInst::BrIf { .. }
            | VInst::FuelCheck { .. }
            | VInst::FStore32 { .. } => {}
            VInst::Call { rets, .. } => {
                for r in rets.vregs(pool) {
                    f(*r);
                }
            }
            VInst::Ret { .. } => {}
        }
    }

    /// All virtual registers referenced as defs or uses (may visit the same index twice).
    pub fn for_each_vreg_touching<F: FnMut(VReg)>(&self, pool: &[VReg], mut f: F) {
        self.for_each_def(pool, &mut f);
        self.for_each_use(pool, &mut f);
    }

    pub fn for_each_use<F: FnMut(VReg)>(&self, pool: &[VReg], mut f: F) {
        match self {
            VInst::AluRRR { src1, src2, .. } => {
                f(*src1);
                f(*src2);
            }
            VInst::AluRRI { src, .. } => f(*src),
            VInst::Icmp { lhs, rhs, .. } => {
                f(*lhs);
                f(*rhs);
            }
            VInst::Select {
                cond,
                if_true,
                if_false,
                ..
            } => {
                f(*cond);
                f(*if_true);
                f(*if_false);
            }
            VInst::Neg { src, .. } | VInst::Bnot { src, .. } | VInst::IcmpImm { src, .. } => {
                f(*src)
            }
            VInst::Mov { src, .. } => f(*src),
            VInst::Load32 { base, .. }
            | VInst::Load8U { base, .. }
            | VInst::Load8S { base, .. }
            | VInst::Load16U { base, .. }
            | VInst::Load16S { base, .. } => f(*base),
            VInst::Store32 { src, base, .. }
            | VInst::Store8 { src, base, .. }
            | VInst::Store16 { src, base, .. } => {
                f(*src);
                f(*base);
            }
            VInst::SlotAddr { .. } => {}
            VInst::MemcpyWords {
                dst_base, src_base, ..
            } => {
                f(*dst_base);
                f(*src_base);
            }
            VInst::IConst32 { .. } | VInst::Label(..) | VInst::Br { .. } => {}
            VInst::BrIf { cond, .. } => f(*cond),
            VInst::Call { args, .. } => {
                for r in args.vregs(pool) {
                    f(*r);
                }
            }
            VInst::Ret { vals, .. } => {
                for r in vals.vregs(pool) {
                    f(*r);
                }
            }
            VInst::FuelCheck { vmctx, .. } => f(*vmctx),

            // Float. The visit ORDER is the contract `regalloc::classes`
            // indexes into — `use_class(inst, i)` answers for the i-th value
            // yielded here — so reordering an arm silently swaps two operands'
            // register files.
            VInst::FAluRRR { src1, src2, .. } => {
                f(*src1);
                f(*src2);
            }
            VInst::Fcmp { lhs, rhs, .. } => {
                f(*lhs);
                f(*rhs);
            }
            VInst::FSelect {
                cond,
                if_true,
                if_false,
                ..
            } => {
                f(*cond);
                f(*if_true);
                f(*if_false);
            }
            VInst::FAluRR { src, .. }
            | VInst::Wfr { src, .. }
            | VInst::Rfr { src, .. }
            | VInst::IToF { src, .. } => f(*src),
            VInst::FLoad32 { base, .. } => f(*base),
            VInst::FStore32 { src, base, .. } => {
                f(*src);
                f(*base);
            }
        }
    }

    pub fn is_call(&self) -> bool {
        matches!(self, VInst::Call { .. })
    }

    pub fn mnemonic(&self) -> &'static str {
        match self {
            VInst::AluRRR { op, .. } => op.mnemonic(),
            VInst::AluRRI { op, .. } => op.mnemonic(),
            VInst::Neg { .. } => "Neg",
            VInst::Bnot { .. } => "Bnot",
            VInst::Icmp { .. } => "Icmp",
            VInst::IcmpImm { .. } => "IcmpImm",
            VInst::Select { .. } => "Select",
            VInst::Br { .. } => "Br",
            VInst::BrIf { .. } => "BrIf",
            VInst::Mov { .. } => "Mov",
            VInst::Load32 { .. } => "Load32",
            VInst::Store32 { .. } => "Store32",
            VInst::Store8 { .. } => "Store8",
            VInst::Store16 { .. } => "Store16",
            VInst::Load8U { .. } => "Load8U",
            VInst::Load8S { .. } => "Load8S",
            VInst::Load16U { .. } => "Load16U",
            VInst::Load16S { .. } => "Load16S",
            VInst::SlotAddr { .. } => "SlotAddr",
            VInst::MemcpyWords { .. } => "MemcpyWords",
            VInst::IConst32 { .. } => "IConst32",
            VInst::Call { .. } => "Call",
            VInst::Ret { .. } => "Ret",
            VInst::Label(..) => "Label",
            VInst::FuelCheck { .. } => "FuelCheck",
            VInst::FAluRRR { op, .. } => op.mnemonic(),
            VInst::FAluRR { op, .. } => op.mnemonic(),
            VInst::Fcmp { .. } => "Fcmp",
            VInst::FSelect { .. } => "FSelect",
            VInst::FLoad32 { .. } => "FLoad32",
            VInst::FStore32 { .. } => "FStore32",
            VInst::Wfr { .. } => "Wfr",
            VInst::Rfr { .. } => "Rfr",
            VInst::IToF { signed: true, .. } => "IToFS",
            VInst::IToF { signed: false, .. } => "IToFU",
        }
    }

    pub fn format_alloc_trace_detail(&self, pool: &[VReg], symbols: &ModuleSymbols) -> String {
        match self {
            VInst::AluRRR {
                op,
                dst,
                src1,
                src2,
                ..
            } => format!("v{} = v{} {} v{}", dst.0, src1.0, op.symbol(), src2.0),
            VInst::AluRRI {
                op, dst, src, imm, ..
            } => format!("v{} = v{} {} {}", dst.0, src.0, op.mnemonic(), imm),
            VInst::Neg { dst, src, .. } => format!("v{} = -v{}", dst.0, src.0),
            VInst::Bnot { dst, src, .. } => format!("v{} = ~v{}", dst.0, src.0),
            VInst::Icmp {
                dst,
                lhs,
                rhs,
                cond,
                ..
            } => format!("v{} = v{} {} v{}", dst.0, lhs.0, icmp_cond_op(*cond), rhs.0),
            VInst::IcmpImm {
                dst,
                src,
                imm,
                cond,
                ..
            } => format!("v{} = (v{} {} {})", dst.0, src.0, icmp_cond_op(*cond), imm),
            VInst::Select {
                dst,
                cond,
                if_true,
                if_false,
                ..
            } => format!(
                "v{} = v{} ? v{} : v{}",
                dst.0, cond.0, if_true.0, if_false.0
            ),
            VInst::Br { target, .. } => format!("Label({target})"),
            VInst::BrIf {
                cond,
                target,
                invert,
                ..
            } => {
                if *invert {
                    format!("!v{}, {}", cond.0, target)
                } else {
                    format!("v{}, {}", cond.0, target)
                }
            }
            VInst::Mov { dst, src, .. } => format!("v{} = v{}", dst.0, src.0),
            VInst::Load32 {
                dst, base, offset, ..
            } => format!("v{} = [v{}{:+}]", dst.0, base.0, offset),
            VInst::Load8U {
                dst, base, offset, ..
            } => format!("v{} = u8[v{}{:+}]", dst.0, base.0, offset),
            VInst::Load8S {
                dst, base, offset, ..
            } => format!("v{} = i8[v{}{:+}]", dst.0, base.0, offset),
            VInst::Load16U {
                dst, base, offset, ..
            } => format!("v{} = u16[v{}{:+}]", dst.0, base.0, offset),
            VInst::Load16S {
                dst, base, offset, ..
            } => format!("v{} = i16[v{}{:+}]", dst.0, base.0, offset),
            VInst::Store32 {
                src, base, offset, ..
            } => format!("[v{}{:+}] = v{}", base.0, offset, src.0),
            VInst::Store8 {
                src, base, offset, ..
            } => format!("[v{}{:+}] = v{} (8)", base.0, offset, src.0),
            VInst::Store16 {
                src, base, offset, ..
            } => format!("[v{}{:+}] = v{} (16)", base.0, offset, src.0),
            VInst::SlotAddr { dst, slot, .. } => format!("v{} = &slot({})", dst.0, slot),
            VInst::MemcpyWords {
                dst_base,
                src_base,
                size,
                ..
            } => format!(
                "memcpy(v{}, v{}, {} words)",
                dst_base.0,
                src_base.0,
                size / 4
            ),
            VInst::IConst32 { dst, val, .. } => format!("v{} = {}", dst.0, val),
            VInst::Call {
                target,
                args,
                rets,
                callee_uses_sret: _,
                caller_passes_sret_ptr: _,
                caller_sret_vm_abi_swap: _,
                ..
            } => {
                let name = symbols.name(*target);
                let args_s = vregs_csv_pool(pool, *args);
                let rets_s = vregs_csv_pool(pool, *rets);
                if rets.count == 0 {
                    format!("{name}({args_s})")
                } else {
                    format!("{rets_s} = {name}({args_s})")
                }
            }
            VInst::Ret { vals, .. } => {
                let s = vregs_csv_pool(pool, *vals);
                format!("({s})")
            }
            VInst::Label(id, _) => format!("({id})"),
            VInst::FuelCheck {
                vmctx,
                decrement,
                trap_label,
                ..
            } => {
                if *decrement {
                    format!("fuel(v{})--, trap -> {}", vmctx.0, trap_label)
                } else {
                    format!("fuel(v{}), trap -> {}", vmctx.0, trap_label)
                }
            }
            VInst::FAluRRR {
                op,
                dst,
                src1,
                src2,
                ..
            } => format!("v{} = v{} {} v{}", dst.0, src1.0, op.symbol(), src2.0),
            VInst::FAluRR { op, dst, src, .. } => {
                format!("v{} = {}(v{})", dst.0, op.mnemonic(), src.0)
            }
            VInst::Fcmp {
                dst,
                lhs,
                rhs,
                cond,
                ..
            } => format!("v{} = v{} {} v{}", dst.0, lhs.0, fcmp_cond_op(*cond), rhs.0),
            VInst::FSelect {
                dst,
                cond,
                if_true,
                if_false,
                ..
            } => format!(
                "v{} = v{} ?.f v{} : v{}",
                dst.0, cond.0, if_true.0, if_false.0
            ),
            VInst::FLoad32 {
                dst, base, offset, ..
            } => format!("v{} = f32[v{}{:+}]", dst.0, base.0, offset),
            VInst::FStore32 {
                src, base, offset, ..
            } => format!("f32[v{}{:+}] = v{}", base.0, offset, src.0),
            VInst::Wfr { dst, src, .. } => format!("v{} = wfr v{}", dst.0, src.0),
            VInst::Rfr { dst, src, .. } => format!("v{} = rfr v{}", dst.0, src.0),
            VInst::IToF {
                dst, src, signed, ..
            } => {
                let kind = if *signed { "i32" } else { "u32" };
                format!("v{} = f32(v{} as {})", dst.0, src.0, kind)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vinst_size() {
        assert!(core::mem::size_of::<VInst>() <= 32);
    }
}
