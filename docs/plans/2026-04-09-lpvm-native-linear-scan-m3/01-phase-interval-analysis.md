## Phase 1: LiveInterval and IntervalAnalysis

### Scope
Create the interval analysis infrastructure: `LiveInterval` struct and the analysis pass that builds intervals from VInst sequences.

### Code Organization
- Place structs and main analysis at top of file
- Helper functions at bottom
- Tests in `mod tests` at top

### Implementation Details

#### LiveInterval struct
```rust
#[derive(Debug, Clone)]
pub struct LiveInterval {
    pub vreg: VReg,
    pub start: u32,
    pub end: u32,
}

impl LiveInterval {
    pub fn new(vreg: VReg) -> Self {
        Self {
            vreg,
            start: u32::MAX,
            end: 0,
        }
    }
}
```

#### IntervalAnalysis
```rust
pub fn analyze_intervals(func: &IrFunction, vinsts: &[VInst]) -> Vec<LiveInterval> {
    // 1. Create interval for each vreg (0..max_vreg)
    // 2. First pass: find first def (start) for each vreg
    // 3. Second pass: find last use (end) for each vreg
    // 4. Return intervals where start != MAX (actually used)
}
```

#### Algorithm
1. Determine max vreg index from `func.vreg_types.len()` and `vinsts` uses/defs
2. Create `Vec<LiveInterval>` with `start=u32::MAX`, `end=0`
3. Iterate vinsts with index `i`:
   - For each def: `intervals[vreg.0].start = min(start, i as u32)`
   - For each use: `intervals[vreg.0].end = max(end, i as u32)`
4. Filter out intervals where `start == u32::MAX` (never defined)

### Tests to Add
```rust
#[test]
fn simple_interval() {
    // vreg defined at 0, used at 5 -> interval (0, 5)
}

#[test]
fn multiple_uses() {
    // vreg defined at 0, used at 2 and 10 -> interval (0, 10)
}

#[test]
fn unused_vreg_filtered() {
    // vreg never defined should not appear in output
}
```

### Validate
```bash
cargo test -p lpvm-native --lib linear_scan
```
