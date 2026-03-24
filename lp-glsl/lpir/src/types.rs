//! Scalar types, virtual registers, and small supporting newtypes.

use core::fmt;

/// LPIR scalar type (width-aware).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum IrType {
    F32,
    I32,
}

impl fmt::Display for IrType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IrType::F32 => write!(f, "f32"),
            IrType::I32 => write!(f, "i32"),
        }
    }
}

/// Dense virtual register index (`v0`, `v1`, …).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Ord, PartialOrd)]
pub struct VReg(pub u32);

impl fmt::Display for VReg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.0)
    }
}

/// Slot index (`ss0`, `ss1`, …).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SlotId(pub u32);

impl fmt::Display for SlotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ss{}", self.0)
    }
}

/// Sub-range of [`crate::module::IrFunction::vreg_pool`] for `Call` / `Return` operands.
#[derive(Clone, Copy, Debug)]
pub struct VRegRange {
    pub start: u32,
    pub count: u16,
}

impl VRegRange {
    pub const EMPTY: Self = Self { start: 0, count: 0 };

    pub fn is_empty(self) -> bool {
        self.count == 0
    }
}

/// Index into the module callee table: imports first, then local functions.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct CalleeRef(pub u32);
