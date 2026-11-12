# Milestone 3: Linear Scan Register Allocation

**Goal**: Replace greedy allocator with linear scan + spill code generation for production-quality register allocation.

## Suggested Plan

`lpvm-native-linear-scan-m3`

## Scope

### In Scope

- **Live interval analysis**: Build intervals from VInst sequence (uses/defs)
- **Linear scan allocation**: Process intervals in order, assign registers or spill
- **Spill slot assignment**: Stack frame layout with assigned spill slots
- **Spill code insertion**: Load before use, store after def for spilled vregs
- **Interval splitting** (basic): Split at calls or high-pressure regions

### Out of Scope

- Coalescing (copy elimination)
- Register preference hints (Callee-saved preference)
- Advanced splitting (beyond call boundaries)
- Graph coloring (future optimization)

## Key Decisions

1. **Allocation order**: Process intervals by start point, allocate free register or spill
2. **Spill heuristic**: Spill interval with highest end point (longest lived)
3. **Call clobbering**: All caller-saved registers clobbered at call sites
4. **Spill code location**: Insert in emit phase using ABI frame offsets

## Deliverables

| Deliverable | Location | Description |
|-------------|----------|-------------|
| `LiveInterval` | `regalloc/linear_scan.rs` | Start/end program points, vreg, uses |
| `IntervalAnalysis` | `regalloc/linear_scan.rs` | Build intervals from VInst sequence |
| `LinearScan` | `regalloc/linear_scan.rs` | Linear scan algorithm implementation |
| `regalloc/mod.rs` update | `regalloc/mod.rs` | Add LinearScan as RegAlloc impl |
| Spill code in emit | `isa/rv32/emit.rs` | Generate sw/lw for spilled intervals |
| Rainbow regression | `filetests/` | Same tests, fewer spills, lower instruction count |

## Dependencies

- M2: Full lowering coverage working with greedy
- Reference: `docs/design/native/overview.md` section on linear scan

## Estimated Scope

- **Lines**: ~600-900
- **Files**: 3-4 modified (`regalloc/linear_scan.rs` new, `emit.rs`, tests)
- **Time**: 3-5 days

## Acceptance Criteria

1. Rainbow filetests pass with linear scan (no regression)
2. Instruction count lower than greedy checkpoint (quantifiable improvement)
3. No spill-related test failures (correctness)
4. Binary size reduction in firmware builds (fewer spill instructions)
