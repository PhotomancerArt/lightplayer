# Custom LPIR→RV32 Backend: Feasibility Analysis

**Date**: 2026-04-07  
**Context**: LightPlayer embedded GLSL JIT on ESP32-C6 (500KB RAM, RISC-V 32-bit)  
**Problem**: Cranelift's peak memory usage limits shader size; binary size is also a concern

---

## Executive Summary

Building a custom single-pass lowerer from LPIR to RV32 machine code is **conceptually straightforward and technically viable**. LPIR's design—structured control flow, scalarized operations, 32-bit-only types—eliminates most complexity that makes Cranelift heavy.

Estimated savings:
- **ROM**: ~100-180 KB (from ~200 KB to ~35-45 KB)
- **Peak RAM during compilation**: ~40-90 KB (from ~50-100 KB to ~2-8 KB)
- **Compile time**: ~5-10x faster (no intermediate representations, no optimizer)

The primary challenge is register allocation. This report analyzes options ranging from greedy allocation (minimal RAM, acceptable code) to linear scan with intervals (moderate RAM, good code).

---

## Why This Is Possible Now

LPIR was designed with this path in mind. Key properties that enable single-pass lowering:

### 1. Structured Control Flow

LPIR has no arbitrary CFG—only `if/else`, `loop`, `switch`, `break`, `continue`, `return`. This maps directly to RISC-V branch instructions without dominator tree construction or CFG analysis.

From current `lpvm-cranelift/src/emit/control.rs`:

```rust
Op::IfStart { cond, .. } => {
    let pred = builder.ins().icmp_imm(IntCC::NotEqual, cond_val, 0);
    builder.ins().brif(pred, then_block, &[], else_block, &[]);
}
```

Direct mapping to RISC-V:
```
beqz cond, else_label
# then body
j merge_label
else_label:
# else body
merge_label:
```

### 2. Scalarized Operations

All vector operations are decomposed before LPIR. Each LPIR opcode maps 1:1 or 1:N to RV32 instructions:

| LPIR Op | RV32 Instructions |
|---------|-------------------|
| `iadd` | `add rd, rs1, rs2` |
| `isub_imm` | `addi rd, rs1, -imm` |
| `imul` | `mul rd, rs1, rs2` (M extension) |
| `ilt_s` | `slt rd, rs1, rs2` |
| `br_if_not` | `beqz rs, exit_label` |
| `call` | `jal ra, offset` |

### 3. Virtual Register Simplicity

LPIR uses virtual registers (vregs) with explicit types (`i32`, `f32`, `ptr`). No SSA—reassignment is allowed. This matches physical registers naturally; we just need to assign x8-x31 efficiently.

---

## Current Cranelift Footprint

| Component | Lines of Code | Est. Binary Size | Est. Peak RAM |
|-----------|--------------|------------------|---------------|
| cranelift-codegen core | 134,000 | ~80-120 KB | ~30-50 KB |
| riscv32 ISA (ISLE) | 7,000 ISLE → Rust | ~30-50 KB | ~10-20 KB |
| regalloc2 (forked) | ~15,000 | ~20-30 KB | ~30-50 KB |
| CLIF builder/verifier | ~20,000 | ~15-25 KB | ~5-10 KB |
| **Total** | **~176,000** | **~150-230 KB** | **~50-100 KB** |

Note: regalloc2 requires significant heap allocation during compilation for its interference graph and union-find structures. This is the primary constraint limiting shader size.

---

## Custom Lowerer Design

### Architecture

```
LPIR (IrFunction)
    │
    ▼
DirectRv32Lowerer
    ├─ Interval Analysis (O(n) with structured CF)
    ├─ Register Allocation (configurable strategy)
    ├─ Instruction Selection (1:1 mapping)
    └─ Instruction Encoding (RV32I/M bytes)
    │
    ▼
JIT Buffer (executable)
```

### Core Components

1. **Interval Analysis** (~300 lines)
   - Single pass over LPIR ops
   - Track first definition and last use per vreg
   - Structured control flow makes this O(n) without iterative dataflow

2. **Register Allocation** (~400-800 lines, see options below)
   - Strategy: greedy, linear scan, or hybrid
   - Target: RV32 registers x8-x31 (24 registers)
   - x0-x7 reserved for special purposes (zero, ra, sp, gp, tp, s0-s1)

3. **Instruction Emitter** (~800 lines)
   - Match each LPIR op, emit RV32 bytes
   - Handle Q32 builtins via `jal` to known addresses
   - Patch forward branches at control-flow merge points

4. **Instruction Encoding** (~500 lines, can extract from Cranelift)
   - R-type, I-type, S-type, B-type, U-type, J-type formats
   - Optional: reuse Cranelift's encoding tables

---

## Register Allocation Options

### Option 1: Greedy Single-Pass (Minimal RAM)

**Memory**: ~1-2 KB peak  
**Code Quality**: ~85-90% of optimal  
**Best For**: Quick implementation, memory-constrained scenarios

```rust
struct GreedyAlloc {
    free_regs: u32,        // Bitmap of x8-x31
    vreg_loc: Vec<Loc>,    // vreg → register or spill slot
    next_spill: u8,        // Next stack slot offset
}

enum Loc { Reg(u8), Spill(u8) }
```

**Algorithm**:
1. Pre-scan: compute next-use distance for each vreg at each PC (lightweight)
2. Forward emission:
   - Need a value? Check if in register
   - Free register available? Use it
   - None free? Spill value with furthest next use (simplified Belady)
3. At loop headers: optionally spill everything (conservative) or save/restore state

**Trade-off**: Some unnecessary spills at loop boundaries, but no complex data structures.

---

### Option 2: Linear Scan with Intervals (Recommended)

**Memory**: ~4-8 KB peak  
**Code Quality**: ~95% of graph coloring  
**Best For**: Production use, balanced quality/memory

Since LPIR has structured control flow, computing live intervals is trivial—no iterative dataflow needed:

```rust
struct Interval {
    vreg: VReg,
    start: u16,    // First definition
    end: u16,      // Last use (computed in single pass)
    loop_depth: u8, // For spill weight
}

struct LinearScan {
    intervals: Vec<Interval>,      // Sorted by start
    active: Vec<(Interval, u8)>,    // (interval, phys_reg)
}
```

**Compute intervals (single pass)**:
```rust
fn compute_intervals(func: &IrFunction) -> Vec<Interval> {
    let mut first_def = vec![None; func.vreg_count];
    let mut last_use = vec![0u16; func.vreg_count];
    
    for (pc, op) in func.body.iter().enumerate() {
        // For each operand:
        // - First time seen? Record as first_def
        // - Always update last_use
        // - At loop headers: extend all live vregs to loop end
        // - At merge points: union intervals from both branches
    }
    
    // Build intervals from first_def/last_use
}
```

Structured control flow means:
- Loop bounds are explicit (`LoopStart`/`End`)
- Merge points are explicit (`IfStart`/`Else`/`End`)
- No irreducible loops → no fixpoint iteration needed

**Allocate**:
1. Sort intervals by start position
2. For each interval:
   - Expire intervals that ended before this start
   - Assign free register, or spill interval ending furthest in future

**Quality**: For shader-like code (mostly straight-line with small loops), this approaches graph coloring quality with 10x less memory.

---

### Option 3: Domain-Specific "Hot/Cold" Split

**Memory**: ~3 KB peak  
**Code Quality**: ~92% of optimal  
**Best For**: Leveraging shader domain knowledge

Observation: GLSL shaders have predictable patterns:
- 2-4 loop induction variables (hot—must stay in registers)
- 1-2 accumulators (color, position—hot)
- Many expression temporaries (cold—can spill/reload)

```rust
struct HybridAlloc {
    hot_regs: [Option<VReg>; 8],    // x8-x15: reserved for hot values
    cold_alloc: GreedyAlloc,         // x16-x31: general allocation
}

fn classify_vreg(op_idx: usize, loop_depth: u8) -> Class {
    if loop_depth > 0 && used_in_loop_header(op_idx) {
        Class::Hot  // Induction variable
    } else if used_in_accumulation(op_idx) {
        Class::Hot  // Accumulator
    } else {
        Class::Cold // Temporary
    }
}
```

Reserve hot registers for the few values that matter most. Spill cold values aggressively.

---

### Comparison Table

| Approach | Peak RAM | Compile Time | Runtime Perf | Code Size | Complexity |
|----------|----------|--------------|--------------|-----------|------------|
| Current regalloc2 | ~50-100 KB | ~5-10 ms | 100% (baseline) | ~30 KB | High (forked) |
| **Option 1 (Greedy)** | ~2 KB | ~0.5 ms | ~85-90% | ~5 KB | Low |
| **Option 2 (Linear)** | ~6 KB | ~1 ms | ~95% | ~8 KB | Medium |
| **Option 4 (Hybrid)** | ~3 KB | ~0.8 ms | ~92% | ~6 KB | Medium |

Note: Runtime performance differences are likely lost in the noise of Q32 builtin function calls (e.g., `__lp_lpir_fmul_q32`), which dominate shader execution time.

---

## Implementation Roadmap

### Phase 1: Proof of Concept (1-2 weeks)

1. **Basic instruction emitter**
   - R-type, I-type encoding for core ALU ops
   - Handle `iadd`, `isub`, `imul`, `icmp`, `br_if`

2. **Greedy allocator (Option 1)**
   - Simple round-robin through x8-x31
   - Spill everything at loops

3. **Single test**: Compile a trivial shader, run in emulator

**Goal**: Validate end-to-end correctness, measure baseline compile time/RAM.

---

### Phase 2: Production Quality (2-3 weeks)

1. **Linear scan allocator (Option 2)**
   - Interval computation from structured LPIR
   - "Furthest end" spill heuristic

2. **Full LPIR opcode coverage**
   - All scalar ops: integer, float (f32 mode), bitwise
   - Control flow: if/else, loop, switch, break, continue
   - Memory: load, store, slot_addr
   - Calls: local functions, Q32 builtins

3. **Correctness testing**
   - Run existing filetests (jit.q32, rv32.q32)
   - Differential testing against Cranelift backend

**Goal**: Matching Cranelift output for all test cases.

---

### Phase 3: Optimization (1-2 weeks)

1. **Profile and optimize**
   - Identify unnecessary moves at block boundaries
   - Add simple coalescing for copies

2. **Memory pressure testing**
   - Compile increasingly large shaders
   - Find actual RAM limits

3. **Integration**
   - Replace Cranelift in `fw-esp32` builds (feature-gated)
   - Keep Cranelift as reference for host builds

**Goal**: Demonstrate 2x+ shader size capability vs. Cranelift on ESP32-C6.

---

## What to Reuse from Cranelift

| Component | Reuse? | Notes |
|-----------|--------|-------|
| RISC-V instruction encoding | **Yes** | Extract `inst/emit.rs` tables (~500 lines) |
| Calling convention logic | **Partial** | Simpler to hand-write for single ABI |
| ISLE | **No** | Overkill for 1:1 mapping |
| Regalloc2 | **No** | Too heavy; replace with linear scan |
| CLIF builder | **No** | Not needed (LPIR is the IR) |

---

## Risks and Mitigations

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Register allocation quality causes visible slowdown | Medium | Profile first; Option 2 should be within 5% of optimal |
| Complex control flow edge cases | Low | LPIR validator ensures well-formedness |
| Q32 builtin ABI mismatch | Low | Reuse existing builtin addresses and signatures |
| Time to implement exceeds benefit | Medium | Start with Option 1 (1 week); measure before committing to full implementation |
| Debugging generated code is harder | Medium | Emit symbol tables; keep Cranelift as reference backend |

---

## Recommendation

Proceed with **Option 2 (Linear Scan)** as the target design, but implement **Option 1 (Greedy)** first as a proof of concept.

The combination of:
- LPIR's structured control flow (enabling O(n) interval analysis)
- Small shader programs (100-300 ops typical)
- 24 available registers (x8-x31)
- Q32 builtins as the performance bottleneck (not register moves)

...makes a custom backend both feasible and likely to outperform Cranelift in the metrics that matter (peak RAM, compile time) with acceptable runtime performance.

---

## Appendix: Why Structured Control Flow Simplifies Everything

Traditional register allocation requires:
1. Build CFG from basic blocks
2. Compute liveness via iterative dataflow
3. Build interference graph
4. Color graph (or allocate with linear scan on sorted intervals)

With structured control flow (LPIR):
1. Single pass records first_def/last_use per vreg
2. At explicit merge points (IfEnd), union live sets
3. At loop headers, extend live ranges to loop end
4. Result: live intervals without dominator trees or bitvectors

This is the key insight that enables linear scan with 1/10th the RAM of regalloc2.
