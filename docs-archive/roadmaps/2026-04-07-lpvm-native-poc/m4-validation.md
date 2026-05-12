# Milestone 4: Validation + Documentation

**Goal**: Verify correctness against Cranelift, measure RAM savings, document next steps.

## Suggested Plan Name

`lpvm-native-m4`

## Scope

### In Scope

- Differential testing: `rv32lp.q32` vs `rv32.q32` (Cranelift) on `op-add.glsl`
- Memory measurement: compile-time peak RAM via custom allocator or `dhat` crate
- Code size comparison: instruction count, bytes emitted
- Document findings: what's better, what's worse, what's next
- Identify follow-up milestones (M5+, beyond POC)

### Explicitly Out of Scope

- No additional filetests (only `op-add.glsl`)
- No optimizations (keep greedy allocator)
- No control flow additions

## Key Decisions

### Differential Testing

Run both backends on same input, compare:

- Output values (must match exactly for Q32)
- Execution trace (if emulator supports tracing)

```rust
#[test]
fn test_differential_op_add() {
    let cranelift_result = run_with_backend(Backend::CraneliftRv32, "op-add.glsl");
    let native_result = run_with_backend(Backend::NativeRv32, "op-add.glsl");
    assert_eq!(cranelift_result, native_result);
}
```

### Memory Measurement

Approach 1: Custom wrapper around allocator

```rust
pub struct CountingAlloc {
    peak: AtomicUsize,
    current: AtomicUsize,
}

// Measure peak allocation during compilation
```

Approach 2: `dhat` crate (DHAT profiler, works in tests)

```rust
#[cfg(feature = "dhat")]
use dhat::DhatAlloc;
```

For POC: Use simple counting allocator wrapper. Accurate enough for order-of-magnitude.

### Metrics to Capture

| Metric                         | Cranelift | Native | Target     |
| ------------------------------ | --------- | ------ | ---------- |
| Compile time RAM               | ~75KB     | ?      | <5KB       |
| Compile time (single function) | ~5ms      | ?      | <1ms       |
| Code size (instructions)       | baseline  | ?      | within 20% |
| Binary size (backend only)     | ~150KB    | ?      | <50KB      |

## Deliverables

| File                                                           | Contents                       |
| -------------------------------------------------------------- | ------------------------------ |
| `docs/reports/2026-04-07-lpvm-native-poc/m5-findings.md` (new) | Results, metrics, comparison   |
| `benches/compile_mem.rs`                                       | Memory benchmark harness       |
| `tests/differential.rs`                                        | Differential test vs Cranelift |

### Updated Documentation

| File                                                | Update                           |
| --------------------------------------------------- | -------------------------------- |
| `docs/reports/2026-04-07-lpvm-native-poc/README.md` | Add M4 results, link to findings |

## Dependencies

- M3 complete (execution working)
- `lpvm-cranelift` for comparison (already in tree)

## Estimated Scope

- ~400 lines (tests, benchmarks, docs)
- 1-2 days
- Complexity: benchmarking methodology, analysis

## Validation

```bash
# Differential test
cargo test -p lpvm-native --test differential

# Memory benchmark
cargo bench -p lpvm-native --test compile_mem -- --nocapture

# Filetest comparison
./scripts/filetests.sh scalar/int/op-add.glsl rv32.q32
./scripts/filetests.sh scalar/int/op-add.glsl rv32lp.q32
# Compare outputs
```

## Success Criteria

**Must have**:

- `op-add.glsl` produces identical output on both backends
- Peak compile RAM < 10KB (10x reduction from Cranelift)

**Nice to have**:

- Compile time < 1ms (5x reduction)
- Code size within 50% of Cranelift (lower bar since unoptimized)

**If failed**:

- Document why (allocator complexity? ELF overhead?)
- Decide whether to continue (fix issues) or abandon

## Follow-up Milestones (Post-POC)

If POC succeeds, potential M5+:

**M5: Control Flow**

- `if/else`, `loop`, `break`, `continue`
- Branch range handling (±4KB limit)
- Label resolution

**M6: Spilling**

- Proper spill slot allocation
- Stack frame optimization
- Handle >24 live values

**M7: Linear Scan Allocator**

- Interval analysis via structured CF
- Upgrade from greedy

**M8: JIT Buffer Output**

- Skip ELF for on-device compilation
- Direct patching of builtin addresses

**M9: Full Filetest Suite**

- Run all `*.glsl` tests
- Fix issues as they appear

**M10: ESP32 On-Device**

- `fw-esp32` integration
- Replace Cranelift in firmware builds
