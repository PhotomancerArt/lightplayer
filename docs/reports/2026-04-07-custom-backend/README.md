# Custom LPIR→RV32 Backend: Report Index

This report analyzes the feasibility of building a custom lightweight backend to replace Cranelift for embedded (ESP32-C6) deployments.

## Documents


| File                                                           | Contents                                                          |
| -------------------------------------------------------------- | ----------------------------------------------------------------- |
| `[00-overview.md](./00-overview.md)`                           | Feasibility analysis, size estimates, register allocation options |
| `[01-interval-analysis.md](./01-interval-analysis.md)`         | O(n) live interval computation for structured control flow        |
| `[02-instruction-selection.md](./02-instruction-selection.md)` | Direct LPIR→RV32 instruction mapping and encoding                 |
| `[03-implementation-plan.md](./03-implementation-plan.md)`     | 4-week phased implementation plan with decision gates             |
| `[04-plumbing-risks.md](./04-plumbing-risks.md)`                 | **Critical: ABI, stack, linking risks and mitigation strategies** |


## Key Findings

### Problem

- Cranelift peak RAM (~50-100KB) limits shader size on ESP32-C6 (500KB total RAM)
- regalloc2's heap-allocated data structures are the primary bottleneck
- Binary size also significant (~150-230KB of ROM)

### Solution

Custom single-pass lowerer leveraging LPIR's structured control flow:


| Metric            | Cranelift   | Custom    | Savings          |
| ----------------- | ----------- | --------- | ---------------- |
| **ROM**           | ~150-230 KB | ~35-45 KB | **~120-185 KB**  |
| **Compile RAM**   | ~50-100 KB  | ~2-8 KB   | **~42-92 KB**    |
| **Compile Time**  | ~5-10 ms    | ~0.5-1 ms | **~5-10x**       |
| **Code Quality**  | Baseline    | ~90-95%   | Acceptable       |
| **Lines of Code** | ~176,000    | ~3,700    | **~47x smaller** |


### Register Allocation Recommendation

**Linear Scan with Intervals** (Option 2):

- Memory: ~4-8 KB peak (vs regalloc2's ~50-100 KB)
- Quality: ~95% of graph coloring for shader-like code
- Enabled by LPIR's structured control flow (no CFG analysis needed)

### Implementation

4-week phased approach:

1. **Week 1**: Greedy allocator PoC (validate correctness)
2. **Week 2**: Linear scan allocator (production quality)
3. **Week 3**: Full feature parity (calls, memory, Q32 builtins)
4. **Week 4**: Optimization and ESP32 integration

Decision gates at each phase allow early termination if approach proves infeasible.

## Why This Is Possible

LPIR was designed with this path in mind:

- **Structured control flow**: Enables O(n) interval analysis without dominator trees
- **Scalarized**: 1:1 or 1:N mapping to RV32 instructions, no pattern matching
- **Non-SSA with virtual registers**: Matches physical registers naturally
- **Mode-agnostic**: Q32/F32 choice at emission time, not in IR

## Recommendation

**Proceed with Phase 1 (1-week PoC)**. The combination of:

- LPIR's constraints enabling simple algorithms
- 24 available registers (x8-x31) for allocation
- Q32 builtin calls dominating shader runtime (not register moves)
- Domain knowledge (small shaders, predictable patterns)

...makes this both feasible and likely to outperform Cranelift in the metrics that matter for embedded (RAM, compile time) while maintaining acceptable runtime performance.

Keep Cranelift as reference backend for host builds and differential testing.