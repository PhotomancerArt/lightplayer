# Linear Scan Register Allocation - Plan Notes

## Scope of Work

Replace the current greedy allocator with a linear scan allocator that:
- Builds live intervals from VInst sequences (def/use analysis)
- Allocates registers by scanning intervals in order of start point
- Spills when no free register available (spill longest-lived interval)
- Handles caller-saved register clobbering at call sites
- Inserts spill code (load before use, store after def) in emit phase

### In Scope
- Live interval analysis
- Linear scan allocation algorithm
- Spill slot assignment in frame layout
- Spill code insertion in emit phase
- Basic interval splitting at calls

### Out of Scope
- Coalescing (copy elimination)
- Register preference hints
- Advanced interval splitting
- Graph coloring

## Current State of Codebase

### Register Allocation (`regalloc/`)
- `mod.rs`: Defines `Allocation` struct with `vreg_to_phys`, `spill_slots`, `incoming_stack_params`, `clobbered`
- `greedy.rs`: Current allocator - round-robin assignment, no liveness, spills when registers exhausted

### Key Structures
- `Allocation::spill_slots: Vec<VReg>` - which vregs are spilled (no physical reg)
- `Allocation::spill_slot(vreg)` - returns slot index for spilled vreg
- `Allocation::incoming_stack_params` - stack params loaded in prologue

### Emission (`isa/rv32/emit.rs`)
- Spill handling uses temps (TEMP0=5/t0, TEMP1=6/t1, TEMP2=7/t2)
- `use_vreg()` - loads spill to temp if needed, returns physical reg
- `def_vreg()` - returns temp for spilled defs, caller must `store_def_vreg()`
- Prologue loads incoming stack params via `incoming_stack_params`

### Frame Layout (`abi/frame.rs`)
- `spill_base_from_sp` - SP-relative offset to first spill slot
- `spill_offset_from_sp(index)` - computes SP-relative offset
- Spill slots are at positive offsets from SP (after outgoing args area)

## Reference: regalloc2 Fastalloc Analysis

The `lp-regalloc2/src/fastalloc/mod.rs` implements a **backward single-pass allocator** (not linear scan):

### Key Characteristics
- **Direction**: Processes instructions from end to beginning of each block
- **Live tracking**: Maintains `live_vregs: VRegSet` - implicitly tracks liveness
- **Allocation order**: For each instruction, allocates uses (late) then defs (early)
- **Eviction**: Uses LRU cache to pick which register to spill when needed
- **Spill slots**: Allocated on demand via `allocstack()`

### Comparison: fastalloc vs Traditional Linear Scan

| Aspect | regalloc2 fastalloc | Traditional Linear Scan |
|--------|---------------------|------------------------|
| Direction | Backward per block | Forward, function-wide |
| Intervals | Implicit (live set) | Explicit pre-built (start, end) |
| Passes | Single pass | Two pass (build + allocate) |
| Spill heuristic | LRU eviction | Spill longest-lived interval |
| Constraints | Handled inline | Usually post-processed |
| Complexity | Lower (no interval building) | Higher (interval analysis) |
| Code quality | Good (fast) | Better (smarter spills) |

### Decision
We'll proceed with **traditional linear scan** (not fastalloc-style) because:
1. Better spill decisions = fewer instructions (important for embedded)
2. Cleaner separation: build intervals, then allocate
3. Easier to reason about and debug
4. Matches the milestone description

---

## Questions to Answer

### 1. Live Interval Representation

**Question**: How should we represent live intervals? Options:
- A: Simple (start_pc, end_pc) per vreg, unified for all uses
- B: Split intervals per program point range (like LLVM's `LiveInterval` with segments)
- C: List of (start, end, use_positions) for detailed heuristics

**Current state**: Greedy allocator has no liveness - it just tracks which vregs need assignment.

**Context**: We need intervals to know when a vreg becomes live (first def) and when it dies (last use). For linear scan, we process intervals ordered by start point.

**Answer**: Use **A** (simple intervals). Recorded in `docs/design/native/future-work.md` for future improvement to segmented intervals and use-position tracking.

### 2. Program Point Numbering

**Question**: How to assign program point numbers to VInsts for interval bounds?

**Current state**: VInsts are in a Vec with indices 0..n-1.

**Context**: Linear scan needs to sort intervals by start point. Using instruction indices (0, 1, 2...) works, but we need to consider:
- Calls should be at specific points for clobbering
- Should we use 2*n numbering (odd for defs, even for uses) for precision?

**Answer**: Use instruction index `i` as the program point. Interval starts at defining instruction, ends at last use.

### 3. Spill Slot Assignment Strategy

**Question**: When and how should spill slots be assigned?

**Current state**: Spill slots assigned during greedy allocation (just push to `spill_slots` Vec).

**Context**: Linear scan may spill different intervals than greedy. We need:
- Track which vregs are spilled
- Assign them to spill slots
- Communicate this to FrameLayout and emit phase

**Suggested course**: 
1. Linear scan produces `spilled_vregs: Vec<VReg>` (determines count)
2. `FrameLayout::compute` already takes `spill_count` parameter
3. Emit phase uses `Allocation::spill_slot(vreg)` to get slot index
4. Same flow as current, just different vregs spilled

### 4. Caller-Saved Register Handling at Calls

**Question**: How should we handle caller-saved registers at call sites?

**Current state**: Greedy marks all caller-saved as clobbered; emit phase saves/restores live values across calls.

**Context**: Linear scan has options:
- A: Mark caller-saved as unavailable for intervals spanning calls (like now, no change to emit)
- B: Implement interval splitting - split at call, use callee-saved for second half

**Answer**: Use **A** - mark caller-saved unavailable for intervals spanning calls. Recorded in `docs/design/native/future-work.md` under "Interval splitting" for future optimization.

### 5. Emit Phase Spill Code Coordination

**Question**: Does emit phase need changes for linear scan spills?

**Current state**: Emit phase handles spills via `use_vreg()`/`def_vreg()`/`store_def_vreg()` pattern using spill temps.

**Context**: The current spill handling should work with linear scan - it just needs to know which vregs are spilled (via `Allocation::is_spilled()`).

**Answer**: No emit phase changes needed for M3. The `Allocation` API (`is_spilled()`, `spill_slot()`, `spill_count()`) already supports arbitrary spill patterns. Linear scan populates the same structure; emit phase consumes it unchanged.

### 6. Testing Strategy

**Question**: How do we validate correctness and measure improvement?

**Current state**: We have perf tests in `lpvm/native/perf/` with instruction counts.

**Context**: Need to ensure:
1. All existing tests still pass (no regression)
2. Instruction counts go down (fewer spills)
3. Spill-related tests specifically show improvement

**Answer**: 
1. Run full filetest suite on linear scan (should pass)
2. Compare instruction counts on `lpvm/native/perf/` tests vs greedy
3. Target: fewer spills = fewer load/store instructions = lower counts

### 7. Integration Point

**Question**: Where does linear scan integrate with existing code?

**Current state**: `emit_function_bytes()` in `isa/rv32/emit.rs` calls `GreedyAlloc::new().allocate_with_func_abi()`.

**Context**: Need to swap allocator. Options:
- A: Add `LinearScan` allocator, switch the call site
- B: Make allocator selectable via compile options

**Answer**: Use **A** - replace greedy entirely for native backend. Keep greedy code for debugging if needed, but goal is replacement.
