//! Register role classification for RISC-V 32-bit registers.
//!
//! This module provides semantic methods to check register categories
//! instead of using magic number ranges.

use super::regs::Gpr;

/// Trait for checking register roles and categories.
pub trait RegisterRole {
    /// Check if this register is an argument register (a0-a7).
    ///
    /// Argument registers are used for passing function arguments
    /// according to the RISC-V calling convention.
    fn is_argument_register(&self) -> bool;

    /// Check if this register is a return register (a0-a7).
    ///
    /// Return registers are used for returning function values.
    /// In RISC-V, these are the same as argument registers.
    fn is_return_register(&self) -> bool;

    /// Check if this register is a temporary register (t0-t6).
    ///
    /// Temporary registers are caller-saved and can be used
    /// for intermediate computations.
    fn is_temporary(&self) -> bool;

    /// Check if this register is a saved register (s0-s11).
    ///
    /// Saved registers are callee-saved and must be preserved
    /// across function calls.
    fn is_callee_saved(&self) -> bool;

    /// Check if this register is caller-saved.
    ///
    /// Caller-saved registers include argument registers (a0-a7),
    /// temporary registers (t0-t6), and the return address (ra).
    fn is_caller_saved(&self) -> bool;
}

impl RegisterRole for Gpr {
    fn is_argument_register(&self) -> bool {
        matches!(
            *self,
            Gpr::A0 | Gpr::A1 | Gpr::A2 | Gpr::A3 | Gpr::A4 | Gpr::A5 | Gpr::A6 | Gpr::A7
        )
    }

    fn is_return_register(&self) -> bool {
        // Return registers are the same as argument registers in RISC-V
        self.is_argument_register()
    }

    fn is_temporary(&self) -> bool {
        matches!(
            *self,
            Gpr::T0 | Gpr::T1 | Gpr::T2 | Gpr::T3 | Gpr::T4 | Gpr::T5 | Gpr::T6
        )
    }

    fn is_callee_saved(&self) -> bool {
        matches!(
            *self,
            Gpr::S0
                | Gpr::S1
                | Gpr::S2
                | Gpr::S3
                | Gpr::S4
                | Gpr::S5
                | Gpr::S6
                | Gpr::S7
                | Gpr::S8
                | Gpr::S9
                | Gpr::S10
                | Gpr::S11
        )
    }

    fn is_caller_saved(&self) -> bool {
        // Caller-saved: argument registers, temporary registers, and return address
        self.is_argument_register() || self.is_temporary() || *self == Gpr::Ra
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_argument_registers() {
        assert!(Gpr::A0.is_argument_register());
        assert!(Gpr::A1.is_argument_register());
        assert!(Gpr::A7.is_argument_register());
        assert!(!Gpr::T0.is_argument_register());
        assert!(!Gpr::S0.is_argument_register());
        assert!(!Gpr::Ra.is_argument_register());
    }

    #[test]
    fn test_return_registers() {
        assert!(Gpr::A0.is_return_register());
        assert!(Gpr::A1.is_return_register());
        assert!(Gpr::A7.is_return_register());
        assert!(!Gpr::T0.is_return_register());
        assert!(!Gpr::S0.is_return_register());
    }

    #[test]
    fn test_temporary_registers() {
        assert!(Gpr::T0.is_temporary());
        assert!(Gpr::T1.is_temporary());
        assert!(Gpr::T6.is_temporary());
        assert!(!Gpr::A0.is_temporary());
        assert!(!Gpr::S0.is_temporary());
    }

    #[test]
    fn test_saved_registers() {
        assert!(Gpr::S0.is_callee_saved());
        assert!(Gpr::S1.is_callee_saved());
        assert!(Gpr::S11.is_callee_saved());
        assert!(!Gpr::A0.is_callee_saved());
        assert!(!Gpr::T0.is_callee_saved());
    }

    #[test]
    fn test_caller_saved() {
        assert!(Gpr::A0.is_caller_saved());
        assert!(Gpr::T0.is_caller_saved());
        assert!(Gpr::Ra.is_caller_saved());
        assert!(!Gpr::S0.is_caller_saved());
        assert!(!Gpr::Sp.is_caller_saved());
    }

    #[test]
    fn test_callee_saved() {
        assert!(Gpr::S0.is_callee_saved());
        assert!(Gpr::S1.is_callee_saved());
        assert!(Gpr::S11.is_callee_saved());
        assert!(!Gpr::A0.is_callee_saved());
        assert!(!Gpr::T0.is_callee_saved());
        assert!(!Gpr::Ra.is_callee_saved());
    }

    #[test]
    fn test_special_registers() {
        // Special registers should not match any category
        assert!(!Gpr::Zero.is_argument_register());
        assert!(!Gpr::Zero.is_temporary());
        assert!(!Gpr::Zero.is_callee_saved());
        assert!(!Gpr::Zero.is_caller_saved());
        assert!(!Gpr::Zero.is_callee_saved());

        assert!(!Gpr::Sp.is_argument_register());
        assert!(!Gpr::Sp.is_temporary());
        assert!(!Gpr::Sp.is_callee_saved());
        assert!(!Gpr::Sp.is_caller_saved());
        assert!(!Gpr::Sp.is_callee_saved());
    }
}
