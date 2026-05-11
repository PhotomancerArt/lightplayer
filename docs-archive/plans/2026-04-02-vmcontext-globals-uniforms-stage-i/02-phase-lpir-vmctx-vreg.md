# Phase 2: Update LPIR to Include vmctx_vreg

## Scope of Phase

Add explicit VMContext representation to `IrFunction`. The first vreg (vreg 0) will always hold the VMContext pointer.

## Code Organization Reminders

- Add the new field near the top of `IrFunction`
- Update constructors to initialize `vmctx_vreg` to `VReg(0)`
- Place helper methods at the bottom

## Implementation Details

### 1. Update `lpir/src/module.rs`

Add `vmctx_vreg` field to `IrFunction`:

```rust
pub struct IrFunction {
    /// VReg holding the VMContext pointer. Always VReg(0).
    pub vmctx_vreg: VReg,
    /// Number of user-visible parameters (not including VMContext)
    pub param_count: u16,
    /// Types for all vregs. vreg_types[0] is the pointer type for vmctx.
    pub vreg_types: Vec<IrType>,
    // ... rest of fields unchanged
}
```

Update `IrFunction::new()` to initialize `vmctx_vreg`:

```rust
impl IrFunction {
    pub fn new(param_count: u16) -> Self {
        let mut func = Self {
            vmctx_vreg: VReg(0),  // NEW: VMContext is always vreg 0
            param_count,
            vreg_types: Vec::new(),
            // ... rest
        };
        
        // Reserve vreg 0 for VMContext (pointer type)
        func.vreg_types.push(IrType::I32);  // Pointer type is I32 on 32-bit targets
        
        // Reserve vregs for user params
        for _ in 0..param_count {
            func.vreg_types.push(IrType::I32);  // Default, will be updated
        }
        
        func
    }
}
```

Add helper methods:

```rust
impl IrFunction {
    /// Get the vreg for a user parameter (0-indexed, not including VMContext)
    pub fn user_param_vreg(&self, user_index: u16) -> VReg {
        assert!(user_index < self.param_count);
        VReg(self.vmctx_vreg.0 + 1 + user_index as u32)
    }

    /// Get the total number of params including VMContext
    pub fn total_param_count(&self) -> u16 {
        1 + self.param_count  // vmctx + user params
    }
}
```

### 2. Update existing code that accesses params

Find all places that iterate `0..param_count` for params and update to account for VMContext:

```rust
// OLD: param 0 is at vreg_types[0]
for i in 0..func.param_count {
    let ty = func.vreg_types[i as usize];
}

// NEW: param 0 is at vreg_types[1] (after vmctx at index 0)
for i in 0..func.param_count {
    let ty = func.vreg_types[func.vmctx_vreg.0 as usize + 1 + i as usize];
}
```

## Tests to Write

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vmctx_vreg_is_zero() {
        let func = IrFunction::new(2);
        assert_eq!(func.vmctx_vreg, VReg(0));
    }

    #[test]
    fn user_params_start_at_vreg_1() {
        let func = IrFunction::new(2);
        assert_eq!(func.user_param_vreg(0), VReg(1));
        assert_eq!(func.user_param_vreg(1), VReg(2));
    }

    #[test]
    fn total_param_count_includes_vmctx() {
        let func = IrFunction::new(3);
        assert_eq!(func.total_param_count(), 4);  // vmctx + 3 user params
    }

    #[test]
    fn vreg_types_has_vmctx_first() {
        let func = IrFunction::new(2);
        assert_eq!(func.vreg_types.len(), 3);  // vmctx + 2 params
        assert_eq!(func.vreg_types[0], IrType::I32);  // vmctx is pointer
    }
}
```

## Validate

```bash
cargo test -p lpir
cargo check -p lpir --target riscv32imac-unknown-none-elf
```

## Notes

- This is a breaking change to LPIR. Any code that assumes param 0 is at vreg 0 will break.
- The next phases (Cranelift, WASM) will handle this correctly since they'll use the new API.
