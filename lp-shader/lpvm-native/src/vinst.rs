//! Virtual instructions: post-lowering, pre-regalloc.
//!
//! Compact layout: [`VReg`] is `u16`; [`VInst::Call`] / [`VInst::Ret`] use [`VRegSlice`] into
//! [`crate::lower::LoweredFunction::vreg_pool`]; callee is [`SymbolId`] into [`ModuleSymbols`].

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// LPIR virtual register at the IR boundary; lowered to [`VReg`] (`u16`).
pub type IrVReg = lpir::VReg;

/// Virtual register index after lowering (`0..`[`crate::config::MAX_VREGS`]).
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
    pub fn intern(&mut self, name: impl Into<String>) -> SymbolId {
        let name = name.into();
        if let Some(i) = self.names.iter().position(|n| *n == name) {
            return SymbolId(i as u16);
        }
        let id = self.names.len();
        assert!(
            id < usize::from(u16::MAX),
            "ModuleSymbols::intern: too many symbols"
        );
        self.names.push(name);
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

fn vregs_csv_pool(pool: &[VReg], slice: VRegSlice) -> String {
    slice
        .vregs(pool)
        .iter()
        .map(|r| format!("v{}", r.0))
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VInst {
    Add32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: u16,
    },
    Sub32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: u16,
    },
    Neg32 {
        dst: VReg,
        src: VReg,
        src_op: u16,
    },
    Mul32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: u16,
    },
    And32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: u16,
    },
    Or32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: u16,
    },
    Xor32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: u16,
    },
    Bnot32 {
        dst: VReg,
        src: VReg,
        src_op: u16,
    },
    Shl32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: u16,
    },
    ShrS32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: u16,
    },
    ShrU32 {
        dst: VReg,
        src1: VReg,
        src2: VReg,
        src_op: u16,
    },
    DivS32 {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        src_op: u16,
    },
    DivU32 {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        src_op: u16,
    },
    RemS32 {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        src_op: u16,
    },
    RemU32 {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        src_op: u16,
    },
    Icmp32 {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        cond: IcmpCond,
        src_op: u16,
    },
    IeqImm32 {
        dst: VReg,
        src: VReg,
        imm: i32,
        src_op: u16,
    },
    Select32 {
        dst: VReg,
        cond: VReg,
        if_true: VReg,
        if_false: VReg,
        src_op: u16,
    },
    Br {
        target: LabelId,
        src_op: u16,
    },
    BrIf {
        cond: VReg,
        target: LabelId,
        invert: bool,
        src_op: u16,
    },
    Mov32 {
        dst: VReg,
        src: VReg,
        src_op: u16,
    },
    Load32 {
        dst: VReg,
        base: VReg,
        offset: i32,
        src_op: u16,
    },
    Store32 {
        src: VReg,
        base: VReg,
        offset: i32,
        src_op: u16,
    },
    SlotAddr {
        dst: VReg,
        slot: u32,
        src_op: u16,
    },
    MemcpyWords {
        dst_base: VReg,
        src_base: VReg,
        size: u32,
        src_op: u16,
    },
    IConst32 {
        dst: VReg,
        val: i32,
        src_op: u16,
    },
    Call {
        target: SymbolId,
        args: VRegSlice,
        rets: VRegSlice,
        callee_uses_sret: bool,
        src_op: u16,
    },
    Ret {
        vals: VRegSlice,
        src_op: u16,
    },
    Label(LabelId, u16),
}

impl VInst {
    /// Index of the originating LPIR op in [`lpir::IrFunction::body`], when tracked.
    pub fn src_op(&self) -> Option<u32> {
        let raw = match self {
            VInst::Add32 { src_op, .. }
            | VInst::Sub32 { src_op, .. }
            | VInst::Neg32 { src_op, .. }
            | VInst::Mul32 { src_op, .. }
            | VInst::And32 { src_op, .. }
            | VInst::Or32 { src_op, .. }
            | VInst::Xor32 { src_op, .. }
            | VInst::Bnot32 { src_op, .. }
            | VInst::Shl32 { src_op, .. }
            | VInst::ShrS32 { src_op, .. }
            | VInst::ShrU32 { src_op, .. }
            | VInst::DivS32 { src_op, .. }
            | VInst::DivU32 { src_op, .. }
            | VInst::RemS32 { src_op, .. }
            | VInst::RemU32 { src_op, .. }
            | VInst::Icmp32 { src_op, .. }
            | VInst::IeqImm32 { src_op, .. }
            | VInst::Select32 { src_op, .. }
            | VInst::Br { src_op, .. }
            | VInst::BrIf { src_op, .. }
            | VInst::Mov32 { src_op, .. }
            | VInst::Load32 { src_op, .. }
            | VInst::Store32 { src_op, .. }
            | VInst::SlotAddr { src_op, .. }
            | VInst::MemcpyWords { src_op, .. }
            | VInst::IConst32 { src_op, .. }
            | VInst::Call { src_op, .. }
            | VInst::Ret { src_op, .. } => *src_op,
            VInst::Label(_, src_op) => *src_op,
        };
        unpack_src_op(raw)
    }

    pub fn for_each_def<F: FnMut(VReg)>(&self, pool: &[VReg], mut f: F) {
        match self {
            VInst::Add32 { dst, .. }
            | VInst::Sub32 { dst, .. }
            | VInst::Neg32 { dst, .. }
            | VInst::Mul32 { dst, .. }
            | VInst::And32 { dst, .. }
            | VInst::Or32 { dst, .. }
            | VInst::Xor32 { dst, .. }
            | VInst::Bnot32 { dst, .. }
            | VInst::Shl32 { dst, .. }
            | VInst::ShrS32 { dst, .. }
            | VInst::ShrU32 { dst, .. }
            | VInst::DivS32 { dst, .. }
            | VInst::DivU32 { dst, .. }
            | VInst::RemS32 { dst, .. }
            | VInst::RemU32 { dst, .. }
            | VInst::Icmp32 { dst, .. }
            | VInst::IeqImm32 { dst, .. }
            | VInst::Select32 { dst, .. }
            | VInst::Mov32 { dst, .. }
            | VInst::Load32 { dst, .. }
            | VInst::SlotAddr { dst, .. }
            | VInst::IConst32 { dst, .. } => f(*dst),
            VInst::Store32 { .. }
            | VInst::MemcpyWords { .. }
            | VInst::Label(..)
            | VInst::Br { .. }
            | VInst::BrIf { .. } => {}
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
            VInst::Add32 { src1, src2, .. }
            | VInst::Sub32 { src1, src2, .. }
            | VInst::Mul32 { src1, src2, .. }
            | VInst::And32 { src1, src2, .. }
            | VInst::Or32 { src1, src2, .. }
            | VInst::Xor32 { src1, src2, .. }
            | VInst::Shl32 { src1, src2, .. }
            | VInst::ShrS32 { src1, src2, .. }
            | VInst::ShrU32 { src1, src2, .. }
            | VInst::DivS32 {
                lhs: src1,
                rhs: src2,
                ..
            }
            | VInst::DivU32 {
                lhs: src1,
                rhs: src2,
                ..
            }
            | VInst::RemS32 {
                lhs: src1,
                rhs: src2,
                ..
            }
            | VInst::RemU32 {
                lhs: src1,
                rhs: src2,
                ..
            }
            | VInst::Icmp32 {
                lhs: src1,
                rhs: src2,
                ..
            } => {
                f(*src1);
                f(*src2);
            }
            VInst::Select32 {
                cond,
                if_true,
                if_false,
                ..
            } => {
                f(*cond);
                f(*if_true);
                f(*if_false);
            }
            VInst::Neg32 { src, .. } | VInst::Bnot32 { src, .. } | VInst::IeqImm32 { src, .. } => {
                f(*src);
            }
            VInst::Mov32 { src, .. } => f(*src),
            VInst::Load32 { base, .. } => f(*base),
            VInst::Store32 { src, base, .. } => {
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
        }
    }

    pub fn is_call(&self) -> bool {
        matches!(self, VInst::Call { .. })
    }

    pub fn mnemonic(&self) -> &'static str {
        match self {
            VInst::Add32 { .. } => "Add32",
            VInst::Sub32 { .. } => "Sub32",
            VInst::Neg32 { .. } => "Neg32",
            VInst::Mul32 { .. } => "Mul32",
            VInst::And32 { .. } => "And32",
            VInst::Or32 { .. } => "Or32",
            VInst::Xor32 { .. } => "Xor32",
            VInst::Bnot32 { .. } => "Bnot32",
            VInst::Shl32 { .. } => "Shl32",
            VInst::ShrS32 { .. } => "ShrS32",
            VInst::ShrU32 { .. } => "ShrU32",
            VInst::DivS32 { .. } => "DivS32",
            VInst::DivU32 { .. } => "DivU32",
            VInst::RemS32 { .. } => "RemS32",
            VInst::RemU32 { .. } => "RemU32",
            VInst::Icmp32 { .. } => "Icmp32",
            VInst::IeqImm32 { .. } => "IeqImm32",
            VInst::Select32 { .. } => "Select32",
            VInst::Br { .. } => "Br",
            VInst::BrIf { .. } => "BrIf",
            VInst::Mov32 { .. } => "Mov32",
            VInst::Load32 { .. } => "Load32",
            VInst::Store32 { .. } => "Store32",
            VInst::SlotAddr { .. } => "SlotAddr",
            VInst::MemcpyWords { .. } => "MemcpyWords",
            VInst::IConst32 { .. } => "IConst32",
            VInst::Call { .. } => "Call",
            VInst::Ret { .. } => "Ret",
            VInst::Label(..) => "Label",
        }
    }

    pub fn format_alloc_trace_detail(&self, pool: &[VReg], symbols: &ModuleSymbols) -> String {
        match self {
            VInst::Add32 {
                dst, src1, src2, ..
            } => format!("v{} = v{} + v{}", dst.0, src1.0, src2.0),
            VInst::Sub32 {
                dst, src1, src2, ..
            } => format!("v{} = v{} - v{}", dst.0, src1.0, src2.0),
            VInst::Neg32 { dst, src, .. } => format!("v{} = -v{}", dst.0, src.0),
            VInst::Mul32 {
                dst, src1, src2, ..
            } => format!("v{} = v{} * v{}", dst.0, src1.0, src2.0),
            VInst::And32 {
                dst, src1, src2, ..
            } => format!("v{} = v{} & v{}", dst.0, src1.0, src2.0),
            VInst::Or32 {
                dst, src1, src2, ..
            } => format!("v{} = v{} | v{}", dst.0, src1.0, src2.0),
            VInst::Xor32 {
                dst, src1, src2, ..
            } => format!("v{} = v{} ^ v{}", dst.0, src1.0, src2.0),
            VInst::Bnot32 { dst, src, .. } => format!("v{} = ~v{}", dst.0, src.0),
            VInst::Shl32 {
                dst, src1, src2, ..
            } => format!("v{} = v{} << v{}", dst.0, src1.0, src2.0),
            VInst::ShrS32 {
                dst, src1, src2, ..
            } => format!("v{} = v{} >> v{}", dst.0, src1.0, src2.0),
            VInst::ShrU32 {
                dst, src1, src2, ..
            } => format!("v{} = v{} >>u v{}", dst.0, src1.0, src2.0),
            VInst::DivS32 { dst, lhs, rhs, .. } => {
                format!("v{} = v{} / v{}", dst.0, lhs.0, rhs.0)
            }
            VInst::DivU32 { dst, lhs, rhs, .. } => {
                format!("v{} = v{} /u v{}", dst.0, lhs.0, rhs.0)
            }
            VInst::RemS32 { dst, lhs, rhs, .. } => {
                format!("v{} = v{} % v{}", dst.0, lhs.0, rhs.0)
            }
            VInst::RemU32 { dst, lhs, rhs, .. } => {
                format!("v{} = v{} %u v{}", dst.0, lhs.0, rhs.0)
            }
            VInst::Icmp32 {
                dst,
                lhs,
                rhs,
                cond,
                ..
            } => format!("v{} = v{} {} v{}", dst.0, lhs.0, icmp_cond_op(*cond), rhs.0),
            VInst::IeqImm32 { dst, src, imm, .. } => {
                format!("v{} = (v{} == {})", dst.0, src.0, imm)
            }
            VInst::Select32 {
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
            VInst::Mov32 { dst, src, .. } => format!("v{} = v{}", dst.0, src.0),
            VInst::Load32 {
                dst, base, offset, ..
            } => format!("v{} = [v{}{:+}]", dst.0, base.0, offset),
            VInst::Store32 {
                src, base, offset, ..
            } => format!("[v{}{:+}] = v{}", base.0, offset, src.0),
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
