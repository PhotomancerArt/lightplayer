# Implementation Plan: Custom LPIR→RV32 Backend

> **⚠️ Critical**: See [`04-plumbing-risks.md`](./04-plumbing-risks.md) for detailed discussion of ABI, stack, and linking complexity. Previous attempt at RV32 backend failed on these "plumbing" issues, not instruction selection. This plan incorporates those lessons via strict simplification.

## Phase 1: Proof of Concept (Week 1)

**Goal**: Validate the approach with minimal investment. Demonstrate that we can emit correct RV32 code for simple LPIR.

### Scope
- Implement only core ALU operations: `iadd`, `isub`, `imul`, `icmp`
- Greedy register allocation (round-robin, spill everything at loops)
- Straight-line code only (no control flow)
- Single test: compile and execute `func @add(v1:i32, v2:i32) -> i32`

### Implementation Steps

1. **Instruction encoder** (~200 lines)
   - R-type, I-type encoding
   - Test: encode `add x1, x2, x3`, verify bytes match `riscv64-unknown-elf-objdump`

2. **Greedy allocator** (~150 lines)
   - Simple vreg→phys_reg mapping (x8-x31 round-robin)
   - No spilling for Phase 1

3. **Single-op emitter** (~200 lines)
   - Handle `iadd`, `isub`, `imul`, immediate variants
   - Emit to fixed buffer

4. **Integration test**
   - Parse LPIR text → lower → execute in `lp-riscv-emu`
   - Compare result with expected

### Success Criteria
- Correct output for simple arithmetic functions
- Peak compile-time RAM < 1KB
- Compile time < 1ms for 50-op functions

### Decision Point
If Phase 1 fails (can't even emit correct code), abandon approach. If it succeeds but code quality is terrible, proceed to Phase 2.

---

## Phase 2: Linear Scan Allocator (Week 2)

**Goal**: Production-quality register allocation with reasonable code quality.

### Scope
- Full interval analysis (O(n) from structured LPIR)
- Linear scan with furthest-end spilling
- All control flow: if/else, loop, break, continue
- All integer ALU operations

### Implementation Steps

1. **Interval analysis** (~300 lines)
   - Single-pass computation of [first_def, last_use] per vreg
   - Loop-aware: extend live ranges through loops
   - If/else merge handling

2. **Linear scan allocator** (~400 lines)
   - Sort intervals by start position
   - Active set management
   - Spill slot allocation
   - Furthest-end heuristic

3. **Control flow emitter** (~400 lines)
   - `IfStart`/`Else`/`End`: branch and label resolution
   - `LoopStart`/`LoopEnd`: backward branches
   - `Break`/`Continue`: jump to loop exit/continue

4. **Label resolution** (~100 lines)
   - Two-pass: first pass emits with placeholders, second patches
   - Or: collect label positions during emission, patch forward branches

### Testing Strategy

```rust
// Differential testing against Cranelift
for test_case in filetests {
    let expected = compile_with_cranelift(&test_case.ir);
    let actual = compile_with_custom(&test_case.ir);
    
    // Run both in emulator
    let expected_result = run_in_emu(&expected);
    let actual_result = run_in_emu(&actual);
    
    assert_eq!(expected_result, actual_result);
    
    // Optional: verify custom is smaller/faster to compile
    assert!(actual.compile_time < expected.compile_time / 2);
}
```

### Success Criteria
- Pass all integer-only filetests
- Peak RAM < 8KB during compilation
- Compile time < 2ms for 200-op functions
- Code size within 20% of Cranelift output

---

## Phase 3: Full Feature Set (Week 3)

**Goal**: Complete parity with current Cranelift backend for all LPIR features.

### Scope
- Memory operations: `load`, `store`, `slot_addr`, `memcpy`
- Calls: local functions, Q32 builtins, imports
- Q32 inline operations (fabs, fneg)
- Float operations (F32 mode, for completeness)

### Implementation Steps

1. **Memory operations** (~300 lines)
   - Stack frame layout
   - `slot_addr` to frame pointer offsets
   - `load`/`store` with 12-bit immediate offsets
   - Spill/reload generation

2. **Calling convention** (~400 lines)
   - Argument passing (a0-a7 for first 8 args)
   - Return values (a0-a1, or sret buffer for >2)
   - Stack setup for non-leaf functions
   - Register save/restore for callee-saved (s0-s11)

3. **Q32 builtin calls** (~200 lines)
   - Emit `jal ra, address` with address from context
   - Handle argument setup (may need moves/reloads)
   - Inline small ops: `fneg`, `fabs`

4. **Function prologue/epilogue** (~150 lines)
   - Allocate frame: `addi sp, sp, -frame_size`
   - Save ra and callee-saved regs
   - Restore and return

### Testing Strategy
- Full filetest suite: jit.q32, wasm.q32, rv32.q32
- Edge cases: multi-return, nested loops, complex expressions
- Performance tests: measure actual shader execution time

### Success Criteria
- Pass 100% of existing filetests
- Peak RAM still < 8KB
- Shader execution performance within 10% of Cranelift

---

## Phase 4: Optimization & Integration (Week 4)

**Goal**: Polish and make it the default for embedded builds.

### Scope
- Simple peephole optimizations
- Profile and tune register allocation
- Integration into `lpvm-cranelift` as alternative backend
- Feature gating (default for embedded, optional for host)

### Optimizations

1. **Peephole (optional)** (~200 lines)
   - `mv x, x` elimination
   - `addi x0, x, 0` → nop
   - Merge consecutive loads/stores where safe

2. **Allocator tuning**
   - Profile actual spill placement
   - Adjust heuristics for shader patterns
   - Consider "hot/cold" split if beneficial

3. **Branch relaxation**
   - B-type branches have ±4KB range
   - Detect overflow, use `j` + inverted condition

### Integration Plan

```rust
// In lpvm-cranelift/src/lib.rs
#[cfg(feature = "custom-rv32")]
mod custom_rv32;

pub fn jit_from_ir(ir: &IrModule, opts: &CompileOptions) -> Result<Module, Error> {
    #[cfg(all(feature = "custom-rv32", not(feature = "std")))]
    {
        // Use custom backend for embedded
        custom_rv32::compile(ir, opts)
    }
    
    #[cfg(not(all(feature = "custom-rv32", not(feature = "std"))))]
    {
        // Use Cranelift (default for host, available on embedded if requested)
        cranelift_backend::compile(ir, opts)
    }
}
```

### Success Criteria
- ESP32-C6 firmware builds and runs
- Can compile shaders 2x larger than with Cranelift
- No regressions in shader execution performance
- Cranelift remains available for host/testing via feature flag

---

## Risk Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Linear scan quality insufficient | Medium | High | Keep Cranelift as reference; profile before optimizing |
| Complex control flow bugs | Medium | High | Extensive differential testing; Cranelift as oracle |
| Performance worse than expected | Low | Medium | Measure early (Phase 1); abort if fundamental |
| Time overruns | Medium | Medium | Strict phase gates; drop to Cranelift if slipping |
| Q32 math precision issues | Low | High | Reuse existing builtins; only codegen changes |

---

## Decision Points

### After Phase 1 (Week 1)
**Go/No-Go**: Can we emit correct RV32 code at all?
- **Go**: Proceed to Phase 2
- **No-Go**: Abandon custom backend, invest in Cranelift optimization

### After Phase 2 (Week 2)
**Quality Check**: Is code quality acceptable?
- Metrics: Spill counts, instruction counts, execution time
- **Acceptable**: Proceed to Phase 3
- **Poor**: Investigate allocator improvements or hybrid approach

### After Phase 3 (Week 3)
**Parity Check**: Does it pass all tests?
- **Pass**: Proceed to Phase 4
- **Fail**: Identify gaps, extend timeline or reduce scope

### After Phase 4 (Week 4)
**Ship Check**: Ready for ESP32 builds?
- **Ready**: Make default for `fw-esp32`
- **Not ready**: Keep as optional feature, revisit next quarter

---

## Effort Estimate

| Phase | Lines of Code | Complexity | Confidence |
|-------|---------------|------------|------------|
| Phase 1: PoC | ~800 | Low | High |
| Phase 2: Allocator | ~1200 | Medium | High |
| Phase 3: Features | ~1100 | Medium | High |
| Phase 4: Polish | ~600 | Low | High |
| **Total** | **~3700** | | |

Compare to:
- Cranelift riscv32 lowering: ~7000 lines of ISLE
- regalloc2: ~15,000 lines
- Current `lpvm-cranelift` emit layer: ~2000 lines

The custom backend should be roughly 1/5th the code complexity of the current Cranelift-based solution.

---

## Alternative: Cranelift Optimization Path

If the custom backend proves infeasible, alternatives to reduce memory:

1. **Shrink regalloc2 further**
   - Replace `ChunkedVec` with more aggressive chunking
   - Disable ION allocator entirely
   - Limit number of passes

2. **Pre-size all Cranelift data structures**
   - Fixed-size arena for Function
   - Pre-allocated VCode buffer
   - Fail compilation early if limits exceeded

3. **Bytecode interpreter fallback**
   - For large shaders that exhaust RAM
   - Slow but predictable
   - Always-available fallback

But given the analysis in this report, the custom backend approach is strongly favored for the embedded use case.
