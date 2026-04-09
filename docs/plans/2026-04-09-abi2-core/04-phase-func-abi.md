# Phase 4: FuncAbi

## Scope

Implement the per-function ABI state in `abi2/func_abi.rs`. This is the central struct that ties together classification results and provides the allocatable register set, precolors, and queries for regalloc and emission.

## Code Organization

- `FuncAbi` struct and impl at top
- Construction (new) follows
- Regalloc interface methods
- Emission interface methods
- Helper for building precolors at bottom
- Tests at end

## Implementation Details

### FuncAbi Struct

```rust
use crate::abi2::{PReg, PregSet, ArgLoc, ReturnMethod};
use crate::abi2::classify::{classify_params, classify_return};
use crate::abi2::rv32;

/// Complete ABI description for one function.
/// 
/// Constructed once from the signature, then queried by:
/// - Regalloc: which registers are available? which vregs are precolored?
/// - Emission: is sret? where do params arrive? how do we return?
pub struct FuncAbi {
    // Classification results
    params: Vec<ArgLoc>,
    return_method: ReturnMethod,
    
    // Derived for regalloc
    allocatable: PregSet,
    precolors: Vec<(u32, PReg)>,  // (vreg_index, physical_register)
    
    // Static sets
    caller_saved: PregSet,
    callee_saved_source: PregSet,  // Available callee-saved registers
}

impl FuncAbi {
    /// Construct from signature and param slot count.
    /// 
    /// `total_param_slots` includes vmctx + all user params.
    pub fn new(sig: &LpsFnSig, total_param_slots: usize) -> Self {
        // First classify return to know if sret
        let return_method = classify_return(sig);
        let is_sret = return_method.is_sret();
        
        // Classify params (sret shifts args to a1+)
        let params = classify_params(sig, is_sret);
        
        // Build allocatable set
        let allocatable = build_allocatable(is_sret);
        
        // Build precolors: map vreg indices to arg registers
        let precolors = build_precolors(&params, total_param_slots, is_sret);
        
        Self {
            params,
            return_method,
            allocatable,
            precolors,
            caller_saved: rv32::CALLER_SAVED,
            callee_saved_source: rv32::CALLEE_SAVED,
        }
    }

    // --- Regalloc Interface ---

    /// Register set available for allocation.
    /// Excludes: zero, ra, sp, fp, spill temps, arg regs, and s1 if sret.
    pub fn allocatable(&self) -> PregSet {
        self.allocatable
    }

    /// Precolored register assignments.
    /// Each entry is (vreg_index, physical_register) that is FIXED.
    pub fn precolors(&self) -> &[(u32, PReg)] {
        &self.precolors
    }

    /// Registers clobbered by an outgoing call.
    pub fn call_clobbers(&self) -> PregSet {
        self.caller_saved
    }

    /// Callee-saved registers that are candidates for allocation.
    pub fn callee_saved_source(&self) -> PregSet {
        self.callee_saved_source
    }

    // --- Emission Interface ---

    /// Returns true if this function uses sret convention.
    pub fn is_sret(&self) -> bool {
        self.return_method.is_sret()
    }

    /// Returns the sret preservation register (s1) if sret, None otherwise.
    pub fn sret_preservation_reg(&self) -> Option<PReg> {
        match &self.return_method {
            ReturnMethod::Sret { preserved_reg, .. } => Some(*preserved_reg),
            _ => None,
        }
    }

    /// Location of the nth parameter component.
    pub fn param_loc(&self, idx: usize) -> Option<ArgLoc> {
        self.params.get(idx).copied()
    }

    /// All parameter locations.
    pub fn param_locs(&self) -> &[ArgLoc] {
        &self.params
    }

    /// Return method (direct or sret).
    pub fn return_method(&self) -> &ReturnMethod {
        &self.return_method
    }

    /// Return locations for direct returns (empty if sret).
    pub fn return_locs(&self) -> &[ArgLoc] {
        match &self.return_method {
            ReturnMethod::Direct { locs } => locs,
            _ => &[],
        }
    }
}
```

### Helper Functions

```rust
/// Build allocatable register set.
/// 
/// Base: t2, s2-s11, t3-t6 (from ALLOCA_BASE)
/// If sret: also exclude s1 (reserved for sret ptr preservation)
fn build_allocatable(is_sret: bool) -> PregSet {
    let mut set = rv32::ALLOCA_BASE;
    if is_sret {
        set.remove(rv32::S1);
    }
    set
}

/// Build precolors for parameter vregs.
/// 
/// `total_param_slots` is 1 + user_params (includes vmctx as vreg 0).
/// For each param slot, assigns it to the corresponding arg register.
/// 
/// Returns: Vec of (vreg_index, PReg) pairs.
fn build_precolors(
    param_locs: &[ArgLoc],
    total_param_slots: usize,
    is_sret: bool,
) -> Vec<(u32, PReg)> {
    let mut precolors = Vec::new();
    
    // Map first total_param_slots params to arg registers
    for (vreg_idx, loc) in param_locs.iter().take(total_param_slots).enumerate() {
        if let ArgLoc::Reg(preg) = loc {
            precolors.push((vreg_idx as u32, *preg));
        }
        // If on stack, no precolor - vreg must be loaded from stack
    }
    
    precolors
}
```

## Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi2::rv32;

    // Helper to make signatures
    fn sig_with_return(ret: LpsType) -> LpsFnSig {
        LpsFnSig {
            name: "test".into(),
            return_type: ret,
            parameters: vec![
                Param { name: "vmctx".into(), ty: LpsType::Pointer, .. },
            ],
        }
    }

    #[test]
    fn direct_return_allocatable_includes_s1() {
        let sig = sig_with_return(LpsType::Scalar(ScalarType::Float));
        let abi = FuncAbi::new(&sig, 1);
        
        assert!(!abi.is_sret());
        assert!(abi.allocatable().contains(rv32::S1));
    }

    #[test]
    fn sret_excludes_s1_from_allocatable() {
        let sig = sig_with_return(LpsType::Mat(4, 4, ScalarType::Float));
        let abi = FuncAbi::new(&sig, 1);
        
        assert!(abi.is_sret());
        assert!(!abi.allocatable().contains(rv32::S1));
    }

    #[test]
    fn direct_return_vmctx_precolored_to_a0() {
        let sig = sig_with_return(LpsType::Scalar(ScalarType::Float));
        let abi = FuncAbi::new(&sig, 1);  // 1 param slot: vmctx
        
        let precolors = abi.precolors();
        assert_eq!(precolors.len(), 1);
        assert_eq!(precolors[0], (0, rv32::A0));  // vmctx vreg 0 -> a0
    }

    #[test]
    fn sret_vmctx_precolored_to_a1() {
        let sig = sig_with_return(LpsType::Mat(4, 4, ScalarType::Float));
        let abi = FuncAbi::new(&sig, 1);  // 1 param slot: vmctx
        
        let precolors = abi.precolors();
        assert_eq!(precolors.len(), 1);
        assert_eq!(precolors[0], (0, rv32::A1));  // vmctx vreg 0 -> a1
        // a0 holds sret ptr, not available for params
    }

    #[test]
    fn sret_preservation_reg_is_s1() {
        let sig = sig_with_return(LpsType::Mat(4, 4, ScalarType::Float));
        let abi = FuncAbi::new(&sig, 1);
        
        assert_eq!(abi.sret_preservation_reg(), Some(rv32::S1));
    }

    #[test]
    fn direct_preservation_reg_is_none() {
        let sig = sig_with_return(LpsType::Scalar(ScalarType::Float));
        let abi = FuncAbi::new(&sig, 1);
        
        assert_eq!(abi.sret_preservation_reg(), None);
    }

    #[test]
    fn param_loc_returns_correct_location() {
        let sig = LpsFnSig {
            name: "test".into(),
            return_type: LpsType::Void,
            parameters: vec![
                Param { name: "a".into(), ty: LpsType::Scalar(ScalarType::Float), .. },
            ],
        };
        let abi = FuncAbi::new(&sig, 1);
        
        assert_eq!(abi.param_loc(0), Some(ArgLoc::Reg(rv32::A0)));
        assert_eq!(abi.param_loc(1), None);  // Out of bounds
    }

    #[test]
    fn call_clobbers_is_caller_saved_set() {
        let sig = sig_with_return(LpsType::Scalar(ScalarType::Float));
        let abi = FuncAbi::new(&sig, 1);
        
        let clobbers = abi.call_clobbers();
        assert!(clobbers.contains(rv32::A0));  // Caller-saved
        assert!(clobbers.contains(rv32::T0));  // Caller-saved
        assert!(!clobbers.contains(rv32::S0)); // Callee-saved
        assert!(!clobbers.contains(rv32::S1)); // Callee-saved
    }

    #[test]
    fn allocatable_excludes_reserved_regs() {
        let sig = sig_with_return(LpsType::Scalar(ScalarType::Float));
        let abi = FuncAbi::new(&sig, 1);
        
        let alloc = abi.allocatable();
        assert!(!alloc.contains(rv32::ZERO));
        assert!(!alloc.contains(rv32::RA));
        assert!(!alloc.contains(rv32::SP));
        assert!(!alloc.contains(rv32::S0));   // Frame pointer
        assert!(!alloc.contains(rv32::T0));    // Spill temp
        assert!(!alloc.contains(rv32::T1));    // Spill temp
        assert!(!alloc.contains(rv32::A0));   // Arg/return
    }
}
```

## Validate

```bash
cargo test -p lpvm-native abi2::func_abi
```

All tests should pass. Verify:
- sret vs direct have correct allocatable sets
- Precolors map vreg 0 correctly (a0 for direct, a1 for sret)
- Param locations match classification
- Reserved registers never in allocatable
