# Cranelift Allocation Reduction Study Report

## Executive Summary

Despite reducing code size through q32 transform optimizations, compilation is still failing due to
memory allocation errors. The panic occurs in `regalloc2::ion::Env::init` when trying to allocate a
BTreeMap (140 bytes), indicating that the compilation process itself is running out of memory, not
just the generated code.

**Key Finding:** The issue is not the size of generated code, but the memory required during
compilation, specifically during register allocation.

## Problem Statement

### Current Situation

- **Panic Location:** `regalloc2::ion::<impl regalloc2::ion::data_structures::Env<F>>::init`
- **Panic Reason:** `memory allocation of 140 bytes failed`
- **Function Being Compiled:** `funcid32` with 62 function declarations and a very large `block0`
  containing 100+ instructions
- **Backtrace:** Shows failure during register allocation phase of compilation

### Root Cause

The function being compiled has:

- 62 function declarations (sig0-sig61, fn0-fn61)
- A massive `block0` with 100+ instructions
- Many virtual registers (525+ based on previous analysis)
- Complex control flow with many function calls

When regalloc2's Ion algorithm initializes, it needs to allocate data structures proportional to the
number of virtual registers and instructions. Even though we've reduced the final code size, the
intermediate representation (VCode) is still very large, causing regalloc2 to require significant
memory.

## Investigation Findings

### 1. Register Allocator Algorithms

Cranelift supports two register allocator algorithms via regalloc2:

**Ion (Backtracking):**

- More sophisticated, better register allocation quality
- Uses backtracking with range splitting for optimal allocation
- Uses more memory (BTreeMap, complex data structures for tracking live ranges)
- Default for `RegallocAlgorithm::Backtracking` (which maps to `"backtracking"` setting)
- **Current algorithm being used** (based on default settings)
- Better for code quality but more memory-intensive

**Fastalloc (Single-Pass):**

- Simpler, faster compilation
- Single-pass algorithm that allocates registers in one pass
- Uses less memory (simpler data structures, no backtracking state)
- Available via `RegallocAlgorithm::SinglePass` (which maps to `"single_pass"` setting)
- **Potentially uses less memory** - worth trying
- May produce more register spills and moves, but acceptable for embedded targets

**Key Difference:**

- Ion builds complex data structures (BTreeMap) to track live ranges and perform backtracking
- Fastalloc uses simpler linear data structures and doesn't need backtracking state
- The 140-byte BTreeMap allocation failure suggests Ion's initialization is too memory-intensive

**Location:**
`/Users/yona/dev/photomancer/feature/lp-cranelift-lp2025/cranelift/codegen/src/machinst/compile.rs:99-102`

```rust
options.algorithm = match b.flags().regalloc_algorithm() {
    RegallocAlgorithm::Backtracking => Algorithm::Ion,
    RegallocAlgorithm::SinglePass => Algorithm::Fastalloc,
};
```

**Setting Name:** `regalloc_algorithm` (enum setting)

- Values: `"backtracking"` (default) or `"single_pass"`
- Defined in:
  `/Users/yona/dev/photomancer/feature/lp-cranelift-lp2025/cranelift/codegen/meta/src/shared/settings.rs:33-45`

### 2. CLIF IR Freeing Strategy

**Current State:**

- `build_jit_executable_memory_optimized()` already frees CLIF IR after compilation
- CLIF Function is freed after `define_function()` completes
- However, the Function is still needed during compilation for error messages

**Limitation:**

- The Function is passed to `compile()` and used throughout the compilation pipeline
- It's referenced in error messages: `log::error!("...\nCLIF for error:\n{f:?}")`
- Cannot be freed until after compilation completes

**Location:**
`/Users/yona/dev/photomancer/lp2025/lp-glsl/lp-glsl-compiler/src/backend/codegen/jit.rs:139-177`

### 3. VCode Size

**Observation:**

- VCode is created during lowering (CLIF → VCode)
- VCode is passed to regalloc2
- VCode size is proportional to the number of instructions and virtual registers
- The large function generates a very large VCode

**Potential Optimization:**

- VCode could potentially be freed after register allocation completes
- However, it's needed for binary emission
- Not a viable optimization point

### 4. Memory Allocation Points During Compilation

**Compilation Pipeline:**

1. CLIF IR creation (Function) - **Already optimized** (freed after compilation)
2. Lowering to VCode - Creates VCode structure
3. Register allocation - **FAILURE POINT** (regalloc2::Env::init)
4. Binary emission - Uses VCode and regalloc result

**Key Insight:** The failure happens during register allocation initialization, before any actual
allocation work begins. This suggests that even the initialization data structures are too large for
available memory.

### 5. regalloc2 Memory Usage

**From Analysis:**

- `regalloc2::Env::init` is 20.9 KiB of code (from size analysis)
- Uses BTreeMap internally (the failing allocation)
- Memory usage scales with number of virtual registers and instructions
- Ion algorithm uses more complex data structures than Fastalloc

**BTreeMap Allocation:**

- The panic shows allocation of 140 bytes for a BTreeMap
- This is likely just the initial node allocation
- More allocations will follow as the BTreeMap grows
- The fact that even 140 bytes fails suggests memory is critically low

## Potential Solutions

### Solution 1: Use Fastalloc Instead of Ion (HIGH PRIORITY)

**Approach:**

- Switch from Ion (backtracking) to Fastalloc (single-pass) register allocator
- Fastalloc uses simpler data structures and less memory
- May have slightly worse register allocation quality, but should still work

**Implementation:**

- Set `regalloc_algorithm` to `"single_pass"` in compiler settings
- The setting accepts `"backtracking"` (default, uses Ion) or `"single_pass"` (uses Fastalloc)
- Add `.set("regalloc_algorithm", "single_pass")` to flag builder

**Implementation Locations:**

1. **For ESP32 JIT compilation** (`lp-glsl/esp32-glsl-jit/src/main.rs:230-234`):
   ```rust
   let mut flag_builder = settings::builder();
   flag_builder.set("opt_level", "none").unwrap();
   flag_builder.set("is_pic", "false").unwrap();
   flag_builder.set("enable_verifier", "false").unwrap();
   flag_builder.set("regalloc_algorithm", "single_pass").unwrap();  // ADD THIS
   let isa_flags = settings::Flags::new(flag_builder);
   ```

2. **For default RISC-V32 flags** (
   `lp-glsl/lp-glsl-compiler/src/backend/target/target.rs:116-141`):
   ```rust
   fn default_riscv32_flags() -> Result<Flags, GlslError> {
       let mut flag_builder = settings::builder();
       flag_builder.set("is_pic", "true")?;
       flag_builder.set("use_colocated_libcalls", "false")?;
       flag_builder.set("enable_multi_ret_implicit_sret", "true")?;
       flag_builder.set("regalloc_algorithm", "single_pass")?;  // ADD THIS
       Ok(settings::Flags::new(flag_builder))
   }
   ```

3. **For default host flags** (
   `lp-glsl/lp-glsl-compiler/src/backend/target/target.rs:145-169`):
   ```rust
   fn default_host_flags() -> Result<Flags, GlslError> {
       let mut flag_builder = settings::builder();
       flag_builder.set("is_pic", "false")?;
       flag_builder.set("use_colocated_libcalls", "false")?;
       flag_builder.set("enable_multi_ret_implicit_sret", "true")?;
       flag_builder.set("regalloc_algorithm", "single_pass")?;  // ADD THIS
       Ok(settings::Flags::new(flag_builder))
   }
   ```

**Expected Impact:**

- Reduces memory usage during register allocation
- Simpler data structures = fewer allocations
- May be sufficient to allow compilation to succeed
- Fastalloc uses simpler algorithms that require less memory for initialization

**Risk:**

- Fastalloc may produce slightly worse code quality (more spills/moves)
- However, for embedded targets, this is often acceptable
- Can be tested to verify code quality is acceptable
- If code quality is unacceptable, can revert to backtracking

**Testing:**

- Compile the failing Perlin function with Fastalloc
- Verify compilation succeeds without memory allocation errors
- Test runtime behavior to ensure code quality is acceptable
- Compare generated code size and performance if needed

### Solution 2: Free CLIF Function Immediately After Lowering (MEDIUM PRIORITY)

**Approach:**

- Modify `compile()` function to free CLIF Function immediately after lowering to VCode
- Keep only VCode for register allocation
- Remove Function from error messages (or make them optional)

**Implementation:**

- Modify
  `/Users/yona/dev/photomancer/feature/lp-cranelift-lp2025/cranelift/codegen/src/machinst/compile.rs`
- Change `compile()` signature to not require `&Function` after lowering
- Make error messages optional or use VCode instead

**Expected Impact:**

- Frees CLIF Function memory before register allocation
- Reduces peak memory usage
- However, Function may not be the largest memory consumer

**Risk:**

- Requires modifying Cranelift fork (upstream compatibility concerns)
- Error messages may be less helpful
- May not be sufficient if VCode is the main memory consumer

**Complexity:** High (requires modifying core Cranelift code)

### Solution 3: Reduce VCode Size Before Register Allocation (LONG-TERM)

**Approach:**

- Optimize the lowering process to generate smaller VCode
- Reduce number of virtual registers
- Simplify instruction sequences

**Implementation:**

- Would require significant changes to lowering logic
- May conflict with code quality goals

**Expected Impact:**

- Reduces memory usage throughout compilation
- However, this may reduce code quality

**Risk:**

- Complex to implement
- May not be worth the effort if other solutions work

**Complexity:** Very High

### Solution 4: Increase Heap Size (WORKAROUND)

**Approach:**

- Increase ESP32 heap size from 300KB to larger value
- Provides more memory for compilation

**Implementation:**

- Modify `/Users/yona/dev/photomancer/lp2025/lp-glsl/esp32-glsl-jit/src/main.rs:56`
- Change `esp_alloc::heap_allocator!(size: 300_000);` to larger value

**Expected Impact:**

- Immediate solution to allow compilation
- However, doesn't address root cause

**Risk:**

- May not be possible if hardware limits memory
- Doesn't solve the underlying problem

**Complexity:** Low (quick workaround)

### Solution 5: Compile Functions Incrementally (LONG-TERM)

**Approach:**

- Break large functions into smaller pieces
- Compile each piece separately
- Link pieces together

**Implementation:**

- Would require significant compiler changes
- May not be feasible for GLSL shaders

**Expected Impact:**

- Reduces peak memory usage per compilation unit
- However, may not be applicable to single large functions

**Risk:**

- Very complex to implement
- May not be applicable to this use case

**Complexity:** Very High

## Recommended Approach

### Immediate Actions (High Priority)

1. **Try Fastalloc Register Allocator**
    - Modify compiler settings to use `SinglePass` algorithm
    - Test if compilation succeeds with Fastalloc
    - Verify code quality is acceptable
    - **Estimated effort:** 1-2 hours
    - **Expected impact:** May solve the problem immediately

2. **Increase Heap Size (if possible)**
    - As a temporary workaround while investigating other solutions
    - Test if larger heap allows compilation
    - **Estimated effort:** 5 minutes
    - **Expected impact:** May provide immediate relief

### Short-term Actions (Medium Priority)

3. **Profile Memory Usage During Compilation**
    - Add memory usage logging at each compilation stage
    - Identify which stage uses most memory
    - **Estimated effort:** 2-4 hours
    - **Expected impact:** Better understanding of memory usage

4. **Investigate Freeing CLIF Function Earlier**
    - Analyze if Function can be freed after lowering
    - Modify error handling to not require Function
    - **Estimated effort:** 4-8 hours
    - **Expected impact:** May reduce peak memory usage

### Long-term Actions (Lower Priority)

5. **Optimize VCode Generation**
    - Reduce virtual register count
    - Simplify instruction sequences
    - **Estimated effort:** 1-2 weeks
    - **Expected impact:** Reduces memory usage throughout compilation

## Code Locations

### Key Files to Modify

1. **Register Allocator Selection:**
   -
   `/Users/yona/dev/photomancer/feature/lp-cranelift-lp2025/cranelift/codegen/src/machinst/compile.rs:99-102`
    - Need to find where compiler settings are configured in lp-glsl-compiler

2. **Heap Size Configuration:**
    - `/Users/yona/dev/photomancer/lp2025/lp-glsl/esp32-glsl-jit/src/main.rs:56`

3. **CLIF IR Freeing:**
   -
   `/Users/yona/dev/photomancer/lp2025/lp-glsl/lp-glsl-compiler/src/backend/codegen/jit.rs:139-177`

4. **Compilation Function:**
   -
   `/Users/yona/dev/photomancer/feature/lp-cranelift-lp2025/cranelift/codegen/src/machinst/compile.rs:18-129`

## Testing Strategy

1. **Test Fastalloc:**
    - Compile the failing Perlin function with Fastalloc
    - Verify compilation succeeds
    - Test runtime behavior to ensure code quality is acceptable

2. **Test Heap Increase:**
    - Increase heap size and test compilation
    - Measure actual memory usage
    - Determine minimum heap size needed

3. **Profile Memory:**
    - Add memory usage logging
    - Identify peak memory usage points
    - Measure impact of each optimization

## Conclusion

The memory allocation failure during compilation is caused by regalloc2's Ion algorithm requiring
too much memory for the large function being compiled. The most promising solution is to switch to
Fastalloc, which uses simpler data structures and less memory. This should be tried first as it's a
simple configuration change with potentially immediate results.

If Fastalloc doesn't solve the problem, increasing heap size (if possible) or investigating earlier
CLIF IR freeing may provide additional memory savings. Long-term optimizations to reduce VCode size
are possible but more complex.
