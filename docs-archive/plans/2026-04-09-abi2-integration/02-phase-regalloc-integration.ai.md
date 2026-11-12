## Scope of Phase

Thread `FuncAbi` through the register allocator and respect precolors/allocatable constraints.

## Code Organization Reminders

- Keep `GreedyAlloc` struct unchanged, add new `allocate_with_abi` method
- Place helper functions (`apply_precolors`, `compute_available_pool`) at bottom
- Update existing tests to use `rv32::func_abi_rv32()`
- Add new tests for sret reservation

## Implementation Details

### Changes to `regalloc/mod.rs`

Update `RegAlloc` trait to include the abi-aware method:

```rust
pub trait RegAlloc {
    /// Allocate registers with ABI constraints.
    /// 
    /// The allocator will:
    /// 1. Assign precolored vregs to their ABI-mandated registers
    /// 2. Only assign from the abi.allocatable() set
    /// 3. Respect sret reservation (s1 unavailable when is_sret())
    fn allocate_with_abi(
        &mut self,
        vinsts: &[VInst],
        vregs: &[VRegInfo],
        abi: &FuncAbi,
    ) -> Allocation;
}
```

Keep the old `allocate` method as a convenience wrapper for non-ABI-aware callers, or mark deprecated if we want to force migration.

### Changes to `regalloc/greedy.rs`

Add the new method implementation:

```rust
impl RegAlloc for GreedyAlloc {
    fn allocate_with_abi(
        &mut self,
        vinsts: &[VInst],
        vregs: &[VRegInfo],
        abi: &FuncAbi,
    ) -> Allocation {
        self.reset();
        
        // 1. Apply precolors from ABI
        self.apply_precolors(abi);
        
        // 2. Compute available register pool
        let available = compute_available_pool(abi);
        
        // 3. Build interference graph and allocate remaining vregs
        // Only use registers from `available` pool
        self.allocate_with_pool(vinsts, vregs, available);
        
        // 4. Convert to Allocation result
        self.build_allocation()
    }
}
```

Add helper methods at bottom of `impl GreedyAlloc`:

```rust
fn apply_precolors(&mut self, abi: &FuncAbi) {
    for (vreg, preg) in abi.precolors() {
        self.assignments.insert(*vreg, Assignment::Reg(*preg));
    }
}

fn compute_available_pool(abi: &FuncAbi) -> PregSet {
    let mut available = abi.allocatable();
    
    // Reserve s1 for sret preservation when applicable
    if abi.is_sret() {
        available.remove(crate::isa::rv32::abi2::S1);
    }
    
    available
}
```

Update the allocation logic to respect the pool:

```rust
fn allocate_with_pool(
    &mut self,
    vinsts: &[VInst],
    vregs: &[VRegInfo],
    available: PregSet,
) {
    // ... existing allocation logic ...
    
    // When selecting a register, only consider available ones
    for vreg in unassigned_vregs {
        if let Some(preg) = self.select_from_pool(vreg, available) {
            self.assignments.insert(vreg, Assignment::Reg(preg));
        } else {
            // Spill
            self.spills.insert(vreg);
        }
    }
}

fn select_from_pool(&self, vreg: VReg, pool: PregSet) -> Option<PReg> {
    // Filter self.interference[vreg] against pool
    // Return first available from pool in round-robin order
    let candidates = pool.difference(self.interference.get(&vreg).copied().unwrap_or(PregSet::EMPTY));
    candidates.iter().next()
}
```

### Tests

Update existing tests and add new ones:

```rust
#[test]
fn greedy_respects_precolors() {
    let vinsts = vec![
        VInst::Param { dst: 0 },  // vmctx
        VInst::Param { dst: 1 },  // first arg
    ];
    let vregs = vec![
        VRegInfo { index: 0, is_param: true },
        VRegInfo { index: 1, is_param: true },
    ];
    
    let sig = LpsFnSig {
        name: "f".into(),
        return_type: LpsType::Float,
        parameters: vec![param("a", LpsType::Float)],
    };
    let abi = rv32::func_abi_rv32(&sig, 2);
    
    let mut alloc = GreedyAlloc::new();
    let result = alloc.allocate_with_abi(&vinsts, &vregs, &abi);
    
    // vreg 0 forced to a0, vreg 1 forced to a1
    assert_eq!(result.get(0), Some(Assignment::Reg(rv32::A0)));
    assert_eq!(result.get(1), Some(Assignment::Reg(rv32::A1)));
}

#[test]
fn greedy_excludes_s1_when_sret() {
    // Create many locals to force register pressure
    let mut vinsts = vec![];
    let mut vregs = vec![];
    
    // Params: vmctx + 8 args to fill a0-a7
    for i in 0..9 {
        vinsts.push(VInst::Param { dst: i });
        vregs.push(VRegInfo { index: i, is_param: true });
    }
    
    // Add 20 more locals to force spills and s1 usage attempt
    for i in 9..29 {
        vinsts.push(VInst::Local { dst: i });
        vregs.push(VRegInfo { index: i, is_param: false });
    }
    
    let sig = LpsFnSig {
        name: "f".into(),
        return_type: LpsType::Vec4,  // sret
        parameters: vec![param("a", LpsType::Float); 8],  // 8 args
    };
    let abi = rv32::func_abi_rv32(&sig, 29);
    
    let mut alloc = GreedyAlloc::new();
    let result = alloc.allocate_with_abi(&vinsts, &vregs, &abi);
    
    // Verify s1 is never assigned to any vreg
    for vreg in 0..29 {
        if let Some(Assignment::Reg(preg)) = result.get(vreg) {
            assert_ne!(preg, rv32::S1, "s1 should be reserved for sret");
        }
    }
}

#[test]
fn greedy_allows_s1_when_not_sret() {
    // Same setup but direct return
    let sig = LpsFnSig {
        name: "f".into(),
        return_type: LpsType::Float,  // direct
        parameters: vec![],
    };
    let abi = rv32::func_abi_rv32(&sig, 20);  // 20 locals
    
    // ... create vinsts with pressure ...
    
    let mut alloc = GreedyAlloc::new();
    let result = alloc.allocate_with_abi(&vinsts, &vregs, &abi);
    
    // s1 should be available for allocation
    let s1_used = (0..20).any(|v| {
        result.get(v) == Some(Assignment::Reg(rv32::S1))
    });
    assert!(s1_used || result.spill_count() > 0, "s1 should be used or everything spills");
}
```

## Validate

```bash
# Run regalloc-specific tests
cargo test -p lpvm-native -- regalloc::

# All tests
cargo test -p lpvm-native

# No warnings
cargo check -p lpvm-native
```
