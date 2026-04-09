//! ABI2: physical register sets, argument/return classification, per-function ABI, and frame layout.

mod frame;
mod func_abi;
mod regset;

pub mod classify;

pub use frame::{FrameLayout, SlotKind};
pub use func_abi::FuncAbi;
pub use regset::{PReg, PregSet, RegClass};
