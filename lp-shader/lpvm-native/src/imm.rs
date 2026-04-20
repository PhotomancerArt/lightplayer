//! Shared RV32 immediate-range predicates used by both lowering and the
//! `fold_immediates` peephole pass.

/// Minimum value representable as a signed 12-bit RISC-V immediate.
pub const IMM12_MIN: i32 = -2048;
/// Maximum value representable as a signed 12-bit RISC-V immediate.
pub const IMM12_MAX: i32 = 2047;

/// Returns true iff `val` fits in a signed 12-bit immediate (`addi`, `andi`,
/// `ori`, `xori`, `slti`, `sltiu`, load/store offset, …).
#[inline]
pub fn fits_imm12(val: i32) -> bool {
    (IMM12_MIN..=IMM12_MAX).contains(&val)
}
