# Phase 2: Create BuiltinTable

## Scope

Create `BuiltinTable` that maps symbol names to function addresses, populated once at startup by iterating over `BuiltinId::all()`.

## Implementation Details

### 1. Create `lpvm-native/src/rt_jit/builtins.rs`

```rust
//! Builtin symbol resolution for JIT compilation.
//!
//! The BuiltinTable is populated once at firmware startup by iterating over
//! all BuiltinIds and looking up their addresses. This provides O(log n)
//! symbol lookup during JIT compilation.

use alloc::collections::BTreeMap;
use lps_builtin_ids::BuiltinId;

/// Maps symbol names to function addresses.
///
/// Populated at startup by iterating BuiltinId::all().
pub struct BuiltinTable {
    symbols: BTreeMap<&'static str, usize>,
}

impl BuiltinTable {
    /// Create empty table.
    pub fn new() -> Self {
        Self {
            symbols: BTreeMap::new(),
        }
    }

    /// Populate table with all builtin addresses.
    ///
    /// Call this once at firmware startup after ensuring builtins are
    /// referenced (to prevent dead code elimination).
    ///
    /// Example:
    /// ```
    /// let mut table = BuiltinTable::new();
    /// table.populate();
    /// ```
    pub fn populate(&mut self) {
        for bid in BuiltinId::all() {
            if let Some(addr) = builtin_address(*bid) {
                self.symbols.insert(bid.name(), addr);
            }
        }
    }

    /// Look up address by symbol name.
    ///
    /// Returns None if symbol not found.
    pub fn lookup(&self, name: &str) -> Option<usize> {
        self.symbols.get(name).copied()
    }

    /// Check if table has any entries.
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Number of symbols in table.
    pub fn len(&self) -> usize {
        self.symbols.len()
    }
}

impl Default for BuiltinTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Get address of a builtin by ID.
///
/// Uses inline match (no codegen needed). Pattern follows cranelift's
/// get_function_pointer_inner but returns address as usize.
fn builtin_address(bid: BuiltinId) -> Option<usize> {
    use lps_builtins::builtins::*;
    
    // SAFETY: All these are #[no_mangle] extern "C" functions
    // They won't be dead-code-eliminated if ensure_builtins_referenced() was called
    let addr: usize = match bid {
        // LPIR builtins
        BuiltinId::LpLpirFaddQ32 => lpir::fadd_q32::__lp_lpir_fadd_q32 as usize,
        BuiltinId::LpLpirFsubQ32 => lpir::fsub_q32::__lp_lpir_fsub_q32 as usize,
        BuiltinId::LpLpirFmulQ32 => lpir::fmul_q32::__lp_lpir_fmul_q32 as usize,
        BuiltinId::LpLpirFdivQ32 => lpir::fdiv_q32::__lp_lpir_fdiv_q32 as usize,
        BuiltinId::LpLpirFsqrtQ32 => lpir::fsqrt_q32::__lp_lpir_fsqrt_q32 as usize,
        BuiltinId::LpLpirFnearestQ32 => lpir::fnearest_q32::__lp_lpir_fnearest_q32 as usize,
        BuiltinId::LpLpirFabsQ32 => lpir::float_misc_q32::__lp_lpir_fabs_q32 as usize,
        BuiltinId::LpLpirFceilQ32 => lpir::float_misc_q32::__lp_lpir_fceil_q32 as usize,
        BuiltinId::LpLpirFfloorQ32 => lpir::float_misc_q32::__lp_lpir_ffloor_q32 as usize,
        BuiltinId::LpLpirFtruncQ32 => lpir::float_misc_q32::__lp_lpir_ftrunc_q32 as usize,
        BuiltinId::LpLpirFminQ32 => lpir::float_misc_q32::__lp_lpir_fmin_q32 as usize,
        BuiltinId::LpLpirFmaxQ32 => lpir::float_misc_q32::__lp_lpir_fmax_q32 as usize,
        BuiltinId::LpLpirFtoiSatSQ32 => lpir::ftoi_sat_q32::__lp_lpir_ftoi_sat_s_q32 as usize,
        BuiltinId::LpLpirFtoiSatUQ32 => lpir::ftoi_sat_q32::__lp_lpir_ftoi_sat_u_q32 as usize,
        BuiltinId::LpLpirItofSQ32 => lpir::itof_s_q32::__lp_lpir_itof_s_q32 as usize,
        BuiltinId::LpLpirItofUQ32 => lpir::itof_u_q32::__lp_lpir_itof_u_q32 as usize,
        
        // GLSL builtins (scalars)
        BuiltinId::LpGlslSinQ32 => glsl::sin_q32::__lps_sin_q32 as usize,
        BuiltinId::LpGlslCosQ32 => glsl::cos_q32::__lps_cos_q32 as usize,
        BuiltinId::LpGlslTanQ32 => glsl::tan_q32::__lps_tan_q32 as usize,
        BuiltinId::LpGlslAsinQ32 => glsl::asin_q32::__lps_asin_q32 as usize,
        BuiltinId::LpGlslAcosQ32 => glsl::acos_q32::__lps_acos_q32 as usize,
        BuiltinId::LpGlslAtanQ32 => glsl::atan_q32::__lps_atan_q32 as usize,
        BuiltinId::LpGlslAtan2Q32 => glsl::atan2_q32::__lps_atan2_q32 as usize,
        BuiltinId::LpGlslSinhQ32 => glsl::sinh_q32::__lps_sinh_q32 as usize,
        BuiltinId::LpGlslCoshQ32 => glsl::cosh_q32::__lps_cosh_q32 as usize,
        BuiltinId::LpGlslTanhQ32 => glsl::tanh_q32::__lps_tanh_q32 as usize,
        BuiltinId::LpGlslAsinhQ32 => glsl::asinh_q32::__lps_asinh_q32 as usize,
        BuiltinId::LpGlslAcoshQ32 => glsl::acosh_q32::__lps_acosh_q32 as usize,
        BuiltinId::LpGlslAtanhQ32 => glsl::atanh_q32::__lps_atanh_q32 as usize,
        BuiltinId::LpGlslExpQ32 => glsl::exp_q32::__lps_exp_q32 as usize,
        BuiltinId::LpGlslExp2Q32 => glsl::exp2_q32::__lps_exp2_q32 as usize,
        BuiltinId::LpGlslLogQ32 => glsl::log_q32::__lps_log_q32 as usize,
        BuiltinId::LpGlslLog2Q32 => glsl::log2_q32::__lps_log2_q32 as usize,
        BuiltinId::LpGlslPowQ32 => glsl::pow_q32::__lps_pow_q32 as usize,
        BuiltinId::LpGlslFmaQ32 => glsl::fma_q32::__lps_fma_q32 as usize,
        BuiltinId::LpGlslLdexpQ32 => glsl::ldexp_q32::__lps_ldexp_q32 as usize,
        BuiltinId::LpGlslModQ32 => glsl::mod_q32::__lps_mod_q32 as usize,
        BuiltinId::LpGlslRoundQ32 => glsl::round_q32::__lps_round_q32 as usize,
        BuiltinId::LpGlslInversesqrtQ32 => glsl::inversesqrt_q32::__lps_inversesqrt_q32 as usize,
        
        // Add remaining builtins as needed...
        // For now, unsupported builtins return None
        _ => return None,
    };
    
    Some(addr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_populate() {
        let mut table = BuiltinTable::new();
        assert!(table.is_empty());
        
        table.populate();
        
        // Should have populated some symbols
        assert!(!table.is_empty());
        
        // Check specific lookups
        assert!(table.lookup("__lp_lpir_fadd_q32").is_some());
        assert!(table.lookup("__lps_sin_q32").is_some());
    }

    #[test]
    fn lookup_unknown() {
        let table = BuiltinTable::new();
        assert!(table.lookup("unknown_symbol").is_none());
    }
}
```

## Code Organization Notes

- Place `builtin_address()` match at bottom of file (helper function)
- Keep match arms organized by category (LPIR, GLSL, LPFX, VM)
- For this phase, implement core builtins only - add rest in later phases

## Validate

```bash
# Check RISC-V target
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf

# Run tests (on host, gated out)
cargo test -p lpvm-native --lib
```

## Next Phase

Once BuiltinTable works, proceed to Phase 3: JitEmitContext for code emission with relocations.
