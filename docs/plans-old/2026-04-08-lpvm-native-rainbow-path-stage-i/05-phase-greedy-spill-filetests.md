## Scope of Phase

Add emergency spill support to greedy allocator and identify/write filetests for validation.

## Code Organization Reminders

- Modify greedy allocator to assign spill slots when registers exhausted
- Track spill count in Allocation
- Create spill pressure test filetest

## Implementation Details

### Updated Allocation

```rust
pub struct Allocation {
    pub vreg_to_phys: Vec<Option<PhysReg>>,
    pub clobbered: BTreeSet<PhysReg>,
    pub spill_slots: Vec<VReg>, // Which vregs are spilled
}

impl Allocation {
    pub fn spill_count(&self) -> u32 {
        self.spill_slots.len() as u32
    }
}
```

### Greedy allocator with spill

```rust
impl RegAlloc for GreedyAlloc {
    fn allocate(&self, func: &IrFunction, vinsts: &[VInst]) -> Result<Allocation, NativeError> {
        let n = func.vreg_types.len();
        let slots = func.total_param_slots() as usize;
        
        let mut vreg_to_phys: Vec<Option<PhysReg>> = vec![None; n];
        let mut spill_slots: Vec<VReg> = Vec::new();
        
        // Assign params to arg regs (unchanged)
        for i in 0..slots.min(n) {
            vreg_to_phys[i] = Some(ARG_REGS[i]);
        }
        
        // Assign defs to allocatable regs, spill when exhausted
        let mut next_alloca = 0usize;
        for inst in vinsts {
            for v in inst.defs() {
                let vi = v.0 as usize;
                if vi >= n || vreg_to_phys[vi].is_some() {
                    continue;
                }
                
                if next_alloca < ALLOCA_REGS.len() {
                    vreg_to_phys[vi] = Some(ALLOCA_REGS[next_alloca]);
                    next_alloca += 1;
                } else {
                    // Spill: no register, assign to spill slot
                    spill_slots.push(v);
                    vreg_to_phys[vi] = None; // Explicitly no register
                }
            }
        }
        
        // Verify all used vregs have allocation or spill
        for inst in vinsts {
            for v in inst.uses() {
                let vi = v.0 as usize;
                if vi < n && vreg_to_phys[vi].is_none() && !spill_slots.contains(&v) {
                    return Err(NativeError::UnassignedVReg(v.0));
                }
            }
        }
        
        // Collect clobbers (unchanged)
        let mut clobbered = BTreeSet::new();
        for inst in vinsts {
            if inst.is_call() {
                clobbered.extend(CALLER_SAVED.iter().copied());
            }
        }
        
        Ok(Allocation {
            vreg_to_phys,
            clobbered,
            spill_slots,
        })
    }
}
```

### Spill test filetest

Create `lp-shader/lps-filetests/filetests/scalar/spill_pressure.glsl`:

```glsl
// test: spill_pressure
//
// Forces heavy register pressure to trigger spilling.
// Each mat4 = 16 scalars. 5 mat4s = 80 values, exceeds available registers.

mat4 test_spill_many_mat4() {
    mat4 a = mat4(1.0);
    mat4 b = mat4(2.0);
    mat4 c = mat4(3.0);
    mat4 d = mat4(4.0);
    mat4 e = mat4(5.0);  // 80 scalars total
    return a + b + c + d + e;
}

// run: test_spill_many_mat4() ~= mat4(15.0)
```

## Tests to Write

```rust
#[test]
fn greedy_spills_when_exhausted() {
    let func = func_with_many_vregs(30); // More than ALLOCA_REGS
    let alloc = GreedyAlloc::new().allocate(&func, &vinsts).expect("alloc");
    assert!(alloc.spill_count() > 0);
}

#[test]
fn spilled_vreg_has_no_phys_reg() {
    let func = func_with_many_vregs(30);
    let alloc = GreedyAlloc::new().allocate(&func, &vinsts).expect("alloc");
    for spilled in &alloc.spill_slots {
        let vi = spilled.0 as usize;
        assert!(alloc.vreg_to_phys[vi].is_none());
    }
}
```

## Validate

```bash
# Unit tests
cargo test -p lpvm-native greedy

# Check spill test file exists and compiles
cargo test -p lps-filetests -- spill_pressure
```
