## Phase 3: Build FastAllocation Output and Integrate

### Scope

Build the `FastAllocation` struct from the allocator state, add the
`USE_FASTALLOC` config flag, and integrate the fastalloc path into the
emitter. Wire everything together end-to-end.

### Code Organization Reminders

- Place `build_allocation` method at the bottom of `FastAllocState`
- Add config flag to `config.rs`
- Update `emit_function_bytes` in `emit.rs` with the new path

### Implementation Details

**Add to `FastAllocState` in `fastalloc.rs`:**

```rust
impl FastAllocState {
    /// Build FastAllocation from allocator state.
    /// 
    /// Walks VInsts forward to produce operand_homes in the correct order
    /// (uses first, then defs, matching emitter expectation).
    fn build_allocation(self, vinsts: &[VInst]) -> Result<FastAllocation, NativeError> {
        let mut operand_homes: Vec<OperandHome> = Vec::new();
        let mut operand_base: Vec<usize> = Vec::with_capacity(vinsts.len());
        
        // Track IConst32 values for rematerialization
        let mut iconst_values: alloc::collections::BTreeMap<VReg, i32> = 
            alloc::collections::BTreeMap::new();
        
        // First pass: collect IConst32 values
        for inst in vinsts {
            if let VInst::IConst32 { dst, val, .. } = inst {
                iconst_values.insert(*dst, *val);
            }
        }
        
        // Second pass: build operand homes
        for inst in vinsts {
            operand_base.push(operand_homes.len());
            
            // Uses
            for u in inst.uses() {
                let home = if let Some(&val) = iconst_values.get(&u) {
                    OperandHome::Remat(val)
                } else if let Some(slot) = self.vreg_spill_slot[u.0 as usize] {
                    OperandHome::Spill(slot)
                } else if let Some(preg) = self.vreg_home[u.0 as usize] {
                    OperandHome::Reg(preg)
                } else {
                    // Shouldn't happen for a valid allocation
                    return Err(NativeError::UnassignedVReg(u.0));
                };
                operand_homes.push(home);
            }
            
            // Defs (skip IConst32 - no home needed)
            for d in inst.defs() {
                if iconst_values.contains_key(&d) {
                    continue; // IConst32 def has no home
                }
                
                let home = if let Some(slot) = self.vreg_spill_slot[d.0 as usize] {
                    OperandHome::Spill(slot)
                } else if let Some(preg) = self.vreg_home[d.0 as usize] {
                    OperandHome::Reg(preg)
                } else {
                    // Default: assume it got a register
                    // This might need refinement based on actual allocation
                    return Err(NativeError::UnassignedVReg(d.0));
                };
                operand_homes.push(home);
            }
        }
        
        Ok(FastAllocation {
            operand_homes,
            operand_base,
            edits: self.edits,
            spill_slot_count: self.next_spill_slot,
            incoming_stack_params: Vec::new(), // TODO: from ABI
        })
    }
}
```

**Update `config.rs`:**

```rust
/// When `true`, use the fast backward-walk allocator.
/// When `false`, use greedy/linear scan.
/// Requires `USE_FAST_ALLOC_EMIT` to also be true for end-to-end.
pub const USE_FASTALLOC: bool = false;
```

**Add fastalloc to `regalloc/mod.rs`:**

```rust
mod fastalloc;
pub use fastalloc::FastAllocator;
```

**Update `emit_function_bytes` in `emit.rs`:**

```rust
pub fn emit_function_bytes(
    func: &lpir::IrFunction,
    ir: &lpir::IrModule,
    module_abi: &ModuleAbi,
    fn_sig: &lps_shared::LpsFnSig,
    float_mode: lpir::FloatMode,
    debug_info: bool,
    alloc_trace: bool,
) -> Result<EmittedFunction, NativeError> {
    let lowered = crate::lower::lower_ops(func, ir, module_abi, float_mode)?;
    let vinsts = &lowered.vinsts;
    let slots = func.total_param_slots() as usize;
    let func_abi = super::abi::func_abi_rv32(fn_sig, slots);
    let is_leaf = !vinsts.iter().any(|v| v.is_call());
    let is_sret = func_abi.is_sret();
    
    // Determine which allocation path to use
    let (fast_alloc, alloc) = if crate::config::USE_FASTALLOC {
        // Fastalloc path
        let num_vregs = func.vreg_types.len().max(
            vinsts.iter()
                .flat_map(|v| v.defs().chain(v.uses()))
                .map(|v| v.0 as usize + 1)
                .max()
                .unwrap_or(0)
        );
        
        // Build initial homes from param_locs
        let param_locs = func_abi.param_locs();
        let mut initial_homes: Vec<(VReg, Option<PhysReg>)> = Vec::new();
        for (i, loc) in param_locs.iter().enumerate() {
            let v = VReg(i as u32);
            let home = match loc {
                crate::abi::ArgLoc::Reg(preg) => Some(preg.hw),
                crate::abi::ArgLoc::Stack { .. } => None,
            };
            initial_homes.push((v, home));
        }
        
        let fast = FastAllocator::allocate(vinsts, num_vregs, &initial_homes)?;
        (Some(fast), None)
    } else {
        // Legacy path
        let alloc = allocate_for_emit(func, vinsts, &func_abi, &lowered.loop_regions, alloc_trace)?;
        (None, Some(alloc))
    };
    
    // ... rest of frame layout and emission
    // Use fast_alloc if present, otherwise use alloc with adapter
```

Actually, the logic is getting complex. Let me simplify:

```rust
    // Determine allocation
    let fast: FastAllocation = if crate::config::USE_FASTALLOC {
        // Fastalloc produces FastAllocation directly
        let num_vregs = /* ... */;
        let initial_homes = /* from ABI */;
        FastAllocator::allocate(vinsts, num_vregs, &initial_homes)?
    } else {
        // Legacy allocator + adapter
        let alloc = allocate_for_emit(...)?;
        let call_save = /* ... */;
        AllocationAdapter::adapt(&alloc, vinsts, call_save)
    };
    
    // Now fast is always FastAllocation - use it
```

### Integration Logic

The key insight: with M2 complete, we can always have a `FastAllocation`
available (either from fastalloc directly or via adapter). The emitter
already knows how to consume `FastAllocation`. So the path is:

```rust
let fast: FastAllocation = if USE_FASTALLOC {
    FastAllocator::allocate(...)?
} else {
    let alloc = allocate_for_emit(...)?;
    AllocationAdapter::adapt(&alloc, vinsts, call_save)
};

// Use fast for emission
```

### Tests

Verify integration:

```bash
# With USE_FASTALLOC=false (adapter path)
scripts/glsl-filetests.sh --target rv32lp lpvm/native

# With USE_FASTALLOC=true (fastalloc path)
# Edit config.rs, then:
scripts/glsl-filetests.sh --target rv32lp lpvm/native
```

Expect: straight-line tests pass with both paths; control flow tests fail
with fastalloc (error), pass with adapter.

### Validate

```bash
cargo check -p lpvm-native
cargo test -p lpvm-native --lib
scripts/glsl-filetests.sh --target rv32lp lpvm/native
```
