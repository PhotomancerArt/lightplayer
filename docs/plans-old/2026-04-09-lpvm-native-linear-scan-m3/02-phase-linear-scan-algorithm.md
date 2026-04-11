## Phase 2: LinearScan Algorithm

### Scope
Implement the core linear scan allocation algorithm that processes intervals and assigns registers or spills.

### Code Organization
- Main `LinearScan` struct and `allocate_with_func_abi` at top
- Helper methods (`expire_intervals`, `spill_interval`) below
- Tests in `mod tests`

### Implementation Details

#### LinearScan struct
```rust
pub struct LinearScan;

impl LinearScan {
    pub const fn new() -> Self { Self }
    
    pub fn allocate_with_func_abi(
        &self,
        func: &IrFunction,
        vinsts: &[VInst],
        abi: &FuncAbi,
    ) -> Result<Allocation, NativeError> {
        // 1. Build intervals
        // 2. Sort by start point
        // 3. Scan and allocate
        // 4. Build Allocation result
    }
}
```

#### Core Algorithm
```rust
fn allocate_intervals(
    &self,
    intervals: &[LiveInterval],
    allocatable: &[PhysReg],
) -> (Vec<Option<PhysReg>>, Vec<VReg>) {
    let mut vreg_to_phys: Vec<Option<PhysReg>> = vec![None; max_vreg];
    let mut spill_slots: Vec<VReg> = Vec::new();
    let mut active: Vec<(u32, PhysReg)> = Vec::new(); // (end, preg)
    
    for interval in intervals {
        // 1. Expire intervals that end before this starts
        while let Some((end, preg)) = active.first() {
            if *end <= interval.start {
                active.remove(0);
            } else {
                break;
            }
        }
        
        // 2. Try to assign register
        let used: BTreeSet<PhysReg> = active.iter().map(|(_, p)| *p).collect();
        if let Some(&preg) = allocatable.iter().find(|p| !used.contains(*p)) {
            vreg_to_phys[interval.vreg.0 as usize] = Some(preg);
            active.push((interval.end, preg));
            active.sort_by_key(|(end, _)| *end);
        } else {
            // 3. Spill: find interval with farthest end in active ∪ {current}
            let spill = self.select_spill(interval, &active);
            if spill == interval.vreg {
                // Spill current
                spill_slots.push(interval.vreg);
            } else {
                // Spill from active, reassign its register to current
                let preg = vreg_to_phys[spill.0 as usize].unwrap();
                vreg_to_phys[spill.0 as usize] = None;
                spill_slots.push(spill);
                vreg_to_phys[interval.vreg.0 as usize] = Some(preg);
                active.retain(|(_, p)| *p != preg);
                active.push((interval.end, preg));
                active.sort_by_key(|(end, _)| *end);
            }
        }
    }
    
    (vreg_to_phys, spill_slots)
}
```

#### Spill Selection
```rust
fn select_spill(&self, current: &LiveInterval, active: &[(u32, PhysReg)]) -> VReg {
    // Find interval with farthest end point
    // Returns vreg to spill
}
```

### Caller-Saved Handling
Mark caller-saved registers as unavailable for intervals that span a call:
```rust
// During interval building, note which intervals span calls
// During allocation, don't assign caller-saved to those intervals
```

### Tests to Add
```rust
#[test]
fn allocates_simple_interval() {
    // 2 intervals, 2 registers -> both assigned
}

#[test]
fn spills_when_exhausted() {
    // 3 intervals, 2 registers -> 1 spilled
}

#[test]
fn spill_longest_interval() {
    // Verify we spill the one with farthest end
}
```

### Validate
```bash
cargo test -p lpvm-native --lib linear_scan
```
