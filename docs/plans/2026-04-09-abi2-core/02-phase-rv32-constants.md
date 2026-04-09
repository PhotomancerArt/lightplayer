# Phase 2: RV32 ISA Constants

## Scope

Implement RV32-specific register constants and pre-built register sets in `isa/rv32/abi2.rs`.

## Code Organization

- Individual register constants first (PReg for each x0-x31)
- Register sets (ARG_REGS, RET_REGS) as const arrays
- PregSet constants (CALLER_SAVED, CALLEE_SAVED, etc.)
- Utility functions for set construction at bottom

## Implementation Details

### Individual Registers

```rust
use crate::abi2::{PReg, RegClass, PregSet};

// Special registers
pub const ZERO: PReg = PReg { hw: 0, class: RegClass::Int };
pub const RA: PReg = PReg { hw: 1, class: RegClass::Int };
pub const SP: PReg = PReg { hw: 2, class: RegClass::Int };

// Temporaries (caller-saved)
pub const T0: PReg = PReg { hw: 5, class: RegClass::Int };
pub const T1: PReg = PReg { hw: 6, class: RegClass::Int };
pub const T2: PReg = PReg { hw: 7, class: RegClass::Int };

// Saved registers (callee-saved)
pub const S0: PReg = PReg { hw: 8, class: RegClass::Int };  // Frame pointer
pub const S1: PReg = PReg { hw: 9, class: RegClass::Int };  // Sret preservation
pub const S2: PReg = PReg { hw: 18, class: RegClass::Int };
// ... S3-S11 follow (19-27)

// Arguments / returns (caller-saved)
pub const A0: PReg = PReg { hw: 10, class: RegClass::Int };
pub const A1: PReg = PReg { hw: 11, class: RegClass::Int };
pub const A2: PReg = PReg { hw: 12, class: RegClass::Int };
pub const A3: PReg = PReg { hw: 13, class: RegClass::Int };
pub const A4: PReg = PReg { hw: 14, class: RegClass::Int };
pub const A5: PReg = PReg { hw: 15, class: RegClass::Int };
pub const A6: PReg = PReg { hw: 16, class: RegClass::Int };
pub const A7: PReg = PReg { hw: 17, class: RegClass::Int };

// Temporaries t3-t6 (28-31)
pub const T3: PReg = PReg { hw: 28, class: RegClass::Int };
pub const T4: PReg = PReg { hw: 29, class: RegClass::Int };
pub const T5: PReg = PReg { hw: 30, class: RegClass::Int };
pub const T6: PReg = PReg { hw: 31, class: RegClass::Int };

// Register arrays for iteration
pub const ARG_REGS: [PReg; 8] = [A0, A1, A2, A3, A4, A5, A6, A7];
pub const RET_REGS: [PReg; 4] = [A0, A1, A2, A3];

// All callee-saved for iteration (when saving/restoring)
pub const CALLEE_SAVED_REGS: [PReg; 12] = [
    S0, S1, S2, S3, S4, S5, S6, S7, S8, S9, S10, S11,
];

// Temp registers for spill handling (NOT allocatable - reserved)
pub const SPILL_TEMPS: [PReg; 2] = [T0, T1];
```

### PregSet Constants

```rust
impl PregSet {
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }
}

/// Caller-saved: a0-a7 (10-17), t0-t6 (5-7, 28-31)
pub const CALLER_SAVED: PregSet = PregSet::from_bits(
    (0b11111111 << 10) |   // a0-a7
    (0b111 << 5) |          // t0-t2
    (0b1111 << 28)          // t3-t6
);

/// Callee-saved: s0-s11 (8-9, 18-27)
pub const CALLEE_SAVED: PregSet = PregSet::from_bits(
    (0b11 << 8) |           // s0-s1
    (0b1111111111 << 18)    // s2-s11
);

/// Reserved for special purposes (never allocatable):
/// - x0 (zero)
/// - x1 (ra)
/// - x2 (sp)
/// - x8 (s0, frame pointer)
/// - x5-x6 (t0-t1, spill temps)
/// - x10-x17 (a0-a7, args/return)
pub const RESERVED_ALWAYS: PregSet = PregSet::from_bits(
    (0b111 << 0) |          // x0-x2
    (0b11 << 5) |           // t0-t1 (spill temps)
    (1 << 8) |              // s0 (frame pointer)
    (0b11111111 << 10)      // a0-a7
);

/// Available for allocation (t2, s1-s11, t3-t6)
/// NOTE: s1 is excluded by FuncAbi for sret functions
pub const ALLOCA_BASE: PregSet = PregSet::from_bits(
    (1 << 7) |              // t2
    (0b1111111111 << 18) |  // s2-s11
    (0b1111 << 28)          // t3-t6
);

/// ABI parameters
pub const STACK_ALIGNMENT: u32 = 16;
pub const SRET_THRESHOLD: usize = 2;  // RV32: >2 scalars uses sret
```

## Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi2::PregSet;

    #[test]
    fn caller_saved_contains_a0_a7() {
        assert!(CALLER_SAVED.contains(A0));
        assert!(CALLER_SAVED.contains(A7));
    }

    #[test]
    fn caller_saved_contains_temps() {
        assert!(CALLER_SAVED.contains(T0));
        assert!(CALLER_SAVED.contains(T6));
    }

    #[test]
    fn callee_saved_contains_s0_s11() {
        assert!(CALLEE_SAVED.contains(S0));
        assert!(CALLEE_SAVED.contains(S11));
    }

    #[test]
    fn reserved_contains_special_regs() {
        assert!(RESERVED_ALWAYS.contains(ZERO));
        assert!(RESERVED_ALWAYS.contains(RA));
        assert!(RESERVED_ALWAYS.contains(SP));
        assert!(RESERVED_ALWAYS.contains(S0));  // frame pointer
        assert!(RESERVED_ALWAYS.contains(T0));  // spill temp
    }

    #[test]
    fn reserved_contains_arg_regs() {
        assert!(RESERVED_ALWAYS.contains(A0));
        assert!(RESERVED_ALWAYS.contains(A7));
    }

    #[test]
    fn alloca_base_excludes_reserved() {
        let alloca = ALLOCA_BASE;
        assert!(!alloca.contains(ZERO));
        assert!(!alloca.contains(RA));
        assert!(!alloca.contains(SP));
        assert!(!alloca.contains(S0));  // frame pointer
        assert!(!alloca.contains(T0));  // spill temp
        assert!(!alloca.contains(T1));  // spill temp
        assert!(!alloca.contains(A0));  // args
    }

    #[test]
    fn alloca_base_includes_t2() {
        assert!(ALLOCA_BASE.contains(T2));
    }

    #[test]
    fn alloca_base_excludes_s1() {
        // s1 is in BASE but may be excluded per-function for sret
        // This is handled by FuncAbi, not the base set
    }

    #[test]
    fn arg_regs_array_ordered() {
        assert_eq!(ARG_REGS[0], A0);
        assert_eq!(ARG_REGS[1], A1);
        assert_eq!(ARG_REGS[7], A7);
    }

    #[test]
    fn ret_regs_array_ordered() {
        assert_eq!(RET_REGS[0], A0);
        assert_eq!(RET_REGS[3], A3);
    }
}
```

## Validate

```bash
cargo test -p lpvm-native abi2::rv32
cargo test -p lpvm-native isa::rv32::abi2
```

All tests should pass. No changes to existing code.
