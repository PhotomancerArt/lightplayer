# M3.1: Memory-Optimized Refactoring - Notes

## Scope of Work

This is a refactoring milestone focused on reducing memory pressure before building the M4 allocator infrastructure. The goal is to shrink the VInst enum from ~88 bytes to ~20 bytes per instruction, eliminate heap allocations in Call/Ret, and build a memory-efficient region tree during lowering.

**DECISION:** Work is happening in `lpvm-native` crate — a clean fork of `lpvm-native`. The old allocators and types remain untouched in the original crate. This eliminates the need to maintain backward compatibility with legacy code.

## Current State

### VInst enum size problem
The `VInst` enum is currently ~88 bytes per variant due to `VInst::Call` containing:
- `target: SymbolRef { name: String }` — 24 bytes + heap allocation
- `args: Vec<VReg>` — 24 bytes + heap allocation
- `rets: Vec<VReg>` — 24 bytes + heap allocation

All other variants (Add32, IConst32, etc.) are padded to this size, wasting ~68 bytes per common instruction.

### VReg type mismatch
- `lpir::VReg` is `VReg(u32)` — 4 bytes, supports 4B+ virtual registers
- VInst uses this directly, but native shaders on 320KB devices never need 4B vregs

### Region tree design
Current M4 plan uses `Box<Region>` and `BTreeSet<VReg>` — both problematic for embedded:
- `Box` = separate heap allocation per node
- `BTreeSet` = ~48-64 bytes per node plus allocator overhead

### defs()/uses() allocation
Both methods currently return `impl Iterator` backed by a freshly allocated `Vec<VReg>` per call. For a 100-instruction shader, this is 100+ heap allocations just for iterating registers.

## Questions and Decisions

### Q1: What is the maximum reasonable VReg count for native shaders?

**Context:** We want to shrink VInst-local VReg from u32 to u16. This limits us to 65,536 virtual registers.

**Decision:** u16 (65,536) is safe. Even 10K vregs would be ~40KB just for type metadata. In practice shaders use dozens to low hundreds of vregs.

**STATUS:** ✅ Confirmed. `VReg(pub u16)` for lpvm-native.

### Q2: Should we intern symbol names per-function or module-level?

**Context:** `VInst::Call` contains `SymbolRef { name: String }`. Multiple calls to the same builtin each allocate their own String.

**Decision:** Module-level symbol table. Functions share many symbols (math builtins). Cranelift does this. Natural fit for how imports work.

**STATUS:** ✅ Confirmed. `ModuleSymbols` with `SymbolId(u16)`.

### Q3: How should defs()/uses() work with VRegSlice?

**Context:** With `VRegSlice { start, count }` for Call args/rets, `defs()` and `uses()` need the vreg_pool to resolve the actual registers.

**Options considered:**
- A: Add pool parameter, return iterator — breaks existing code
- B: Return indices with SmallVec — still allocates
- C: Callback-based visitor — zero allocation

**Decision:** Option C. `for_each_def(&self, pool, f)` and `for_each_use(&self, pool, f)`. Zero allocation, clean for VRegSlice.

**STATUS:** ✅ Confirmed.

### Q4: Should RegSet be fixed-size [u64; 4] or dynamic Vec<u64>?

**Context:** We need a bitset type for liveness. Fixed-size is stack-friendly; dynamic is more flexible.

**Decision:** Fixed `[u64; 4]` with constant in config.rs (`MAX_VREGS = 256`). 32 bytes, zero heap. Assert at lowering if exceeded. Easy to expand to 512 if needed.

**STATUS:** ✅ Confirmed.

### Q5: What about src_op: Option<u32>?

**Context:** Every VInst carries `src_op: Option<u32>` for debug tracing back to LPIR. This is 8 bytes per instruction.

**Decision:** Shrink to `u16` with `0xFFFF` as "none" sentinel. Saves 4-6 bytes per variant. LPIR functions on-device won't have 65K ops.

**STATUS:** ✅ Confirmed.

## Summary of Decisions

| Aspect | Old | New | Savings |
|--------|-----|-----|---------|
| VReg | `u32` | `u16` | 2 bytes per register field |
| VInst enum | ~88 bytes | ~20 bytes | ~68 bytes per instruction |
| Call/Ret args | `Vec<VReg>` (2× heap) | `VRegSlice` into pool | 0 heap allocs |
| Callee target | `SymbolRef { String }` | `SymbolId(u16)` | 0 heap allocs per call |
| src_op | `Option<u32>` (8 bytes) | `u16` sentinel (2 bytes) | 6 bytes per VInst |
| defs()/uses() | `Vec<VReg>` alloc per call | callback, zero alloc | 0 heap allocs |
| RegSet | `BTreeSet<VReg>` | `[u64; 4]` bitset | ~2KB → 32 bytes |
| Symbol table | per-Call String | ModuleSymbols pool | N allocs → 1 alloc |

## Notes

- Work is in `lp-shader/lpvm-native/` — clean fork, no legacy compatibility needed
- The old `lpvm-native` crate remains untouched with working greedy/linear allocators
- This refactoring blocks M4 (allocator shell). M4 should assume the new types exist.
- The region tree in M4 will use `RegionId(u16)` into `Vec<Region>` arena.
- The RegSet type should be defined in this milestone so M4 just imports it.

## Success Criteria

1. `VInst` enum size reduced from ~88 bytes to ~20 bytes
2. `Call` and `Ret` variants use pooled VRegSlice instead of Vec<VReg>
3. `SymbolRef` replaced with `SymbolId(u16)` into module-level `ModuleSymbols`
4. VInst-local `VReg` type is `u16` (separate from lpir::VReg which stays u32)
5. `RegSet` type defined as `[u64; 4]` bitset with `MAX_VREGS` constant in config.rs
6. `defs()`/`uses()` replaced with `for_each_def()`/`for_each_use()` (no allocation)
7. `LoweredFunction` and `LoweredModule` carry vreg_pool and module_symbols
8. Region tree arena structure defined (actual building happens in M4)
9. All code in `lpvm-native` compiles and tests pass
