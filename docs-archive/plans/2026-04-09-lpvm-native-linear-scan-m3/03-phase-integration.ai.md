## Phase 3: Integration with Emit Phase

### Scope
Integrate LinearScan into the compilation pipeline by replacing the greedy allocator call in `emit_function_bytes()`.

### Code Organization
- Add `pub use linear_scan::LinearScan` in `regalloc/mod.rs`
- Swap allocator in `isa/rv32/emit.rs`

### Implementation Details

#### regalloc/mod.rs
```rust
pub mod linear_scan;
pub use linear_scan::LinearScan;
```

#### isa/rv32/emit.rs
In `emit_function_bytes()`:
```rust
use crate::regalloc::LinearScan;

// ...

// let alloc = GreedyAlloc::new().allocate_with_func_abi(func, &vinsts, &func_abi)?;
let alloc = LinearScan::new().allocate_with_func_abi(func, &vinsts, &func_abi)?;
```

### Validation
```bash
# Test single file
cargo run -p lps-filetests-app -- test lpvm/native/native-call-simple.glsl -t rv32lp.q32

# Test all native tests
cargo run -p lps-filetests-app -- test lpvm/native/ -t rv32lp.q32
```

### Expected Results
- All tests should pass
- Instruction counts may be slightly different (should be lower or similar)
