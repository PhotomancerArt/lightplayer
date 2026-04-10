# Linear Scan Register Allocation - Design

## Scope of Work

Replace the greedy allocator with a linear scan allocator that:
- Builds live intervals from VInst sequences (def/use analysis)
- Allocates registers by scanning intervals in order of start point
- Spills when no free register available (spill longest-lived interval)
- Handles caller-saved register clobbering at call sites

## File Structure

```
lp-shader/lpvm-native/src/
└── regalloc/
    ├── mod.rs                    # UPDATE: Add LinearScan to exports
    ├── greedy.rs                 # EXISTING: Keep for reference
    └── linear_scan.rs            # NEW: Linear scan implementation

lpvm-native/src/isa/rv32/
└── emit.rs                       # NO CHANGE: Allocation API unchanged
```

## Conceptual Architecture

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────┐
│  VInst Sequence │────▶│ IntervalAnalysis │────▶│ LiveIntervals│
└─────────────────┘     └──────────────────┘     └─────────────┘
                                                          │
                                                          ▼
┌─────────────┐     ┌─────────────┐     ┌──────────────────┐
│  Allocation │◀────│ LinearScan  │◀────│ Sorted by start  │
│ (vreg→phys) │     │  algorithm  │     │      point       │
└─────────────┘     └─────────────┘     └──────────────────┘
```

## Main Components

### LiveInterval
```rust
struct LiveInterval {
    vreg: VReg,
    start: u32,  // Program point where vreg becomes live (first def)
    end: u32,    // Program point where vreg dies (last use)
}
```

### IntervalAnalysis
- Single pass over VInst sequence
- For each vreg: track first def (start) and last use (end)
- Produces `Vec<LiveInterval>`

### LinearScan
- Sort intervals by `start` point
- Maintain `active` set: intervals currently assigned to registers
- For each interval:
  1. Expire intervals where `end < current.start`
  2. If free register available: assign it
  3. Else: spill interval with farthest `end` (longest-lived)

### Spill Heuristic
Spill the interval with the **farthest end point**. This minimizes the number of reloads needed.

## Integration

In `emit_function_bytes()`, replace:
```rust
// Old:
let alloc = GreedyAlloc::new().allocate_with_func_abi(func, &vinsts, &func_abi)?;

// New:
let alloc = LinearScan::new().allocate_with_func_abi(func, &vinsts, &func_abi)?;
```

The `Allocation` struct is unchanged - emit phase works without modification.

## Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Interval representation | Simple (start, end) | Sufficient for M3; segmented can be added later |
| Program points | Instruction indices | Simple, sufficient precision |
| Caller-saved handling | Mark unavailable for intervals spanning calls | Keeps emit phase unchanged |
| Spill heuristic | Spill longest-lived interval | Classic linear scan heuristic |
| Emit phase changes | None | Allocation API already supports arbitrary spills |

## Success Criteria

1. All filetests pass (no regression)
2. Instruction counts on `lpvm/native/perf/` tests are lower than greedy
3. `mat4-reg-pressure.glsl` shows significant improvement (fewer spills)
