# Plan: ABI2 Integration into Compiler Pipeline

## Scope of Work

Wire the new abi2 module into the actual compiler pipeline:
1. API ergonomics - add helper methods to FuncAbi for easier integration
2. Register allocator - respect precolors and allocatable sets from FuncAbi
3. Emitter - handle sret in prologue and VInst::Ret
4. Runtime/Caller - allocate sret buffers and shift arguments

## Current State

**ABI2 is built but unused:**
- `abi2::FuncAbi` has correct RV32 classification (sret for >2 scalars)
- `abi2::FrameLayout` computes stack layout
- `isa/rv32/abi2::func_abi_rv32()` constructs FuncAbi from LpsFnSig
- No code in the actual compile/emit path uses abi2 yet

**Current compile flow (unaffected by abi2):**
```
IrFunction ──► lower ──► VInsts ──► greedy regalloc ──► emit ──► bytes
```

**Integration points needed:**
1. Regalloc needs `FuncAbi` to know precolors and allocatable set
2. Emitter needs `FuncAbi` + `FrameLayout` for prologue/epilogue
3. Runtime needs `FuncAbi` to handle sret buffer allocation

## Questions

### Q1: Should we add helper methods to FuncAbi?

**Context:** Current API requires manual search through precolors slice and matching on ReturnMethod variants. Integration code will repeatedly need:
- Check if vreg has a precolor
- Get sret word count (not just preservation reg)
- Get stack alignment

**Suggested helpers:**
```rust
pub fn precolor_of(&self, vreg: u32) -> Option<PReg>
pub fn sret_word_count(&self) -> Option<u32>
pub fn stack_alignment(&self) -> u32  // or frame_alignment
```

**Options:**
- Add them now (small change, improves ergonomics)
- Skip and add when integration reveals the pain points

**My suggestion:** Add them in Phase 1 - they're trivial and will make subsequent phases cleaner.

### Q2: Regalloc interface - take FuncAbi or just precolors/allocatable?

**Context:** `GreedyAlloc` currently takes `&[VRegInfo]` and makes up its own allocation strategy. It needs to:
- Force parameter vregs into their ABI registers (precolors)
- Only assign from the allocatable set
- Reserve s1 for sret preservation

**Options:**
- Pass `FuncAbi` to regalloc (cleaner, but creates dependency)
- Extract just the sets and pass those (more flexible, more boilerplate)

**My suggestion:** Pass `&FuncAbi` - the regalloc is fundamentally about satisfying ABI constraints, so the dependency is natural.

### Q3: How to thread FuncAbi through the pipeline?

**Context:** Currently emission takes `vinsts: &[VInst]` and `alloc: &Allocation`. To use abi2, we need `FuncAbi` at emit time.

**Options:**
- Thread through `emit_function` signature: `emit(vinsts, alloc, func_abi)`
- Store in `Allocation` struct (but Allocation is regalloc output)
- Look up from module metadata using function name

**My suggestion:** Thread through signature - explicit is better, and the caller already has the signature.

### Q4: FrameLayout stack alignment source?

**Context:** `FrameLayout::compute` takes an `&FuncAbi` but currently ignores it for alignment. RV32 needs 16-byte alignment.

**Options:**
- Add `stack_alignment()` method to FuncAbi
- Pass alignment as separate parameter to FrameLayout::compute
- Hardcode 16 in FrameLayout (RV32-specific)

**My suggestion:** Add to FuncAbi - frame layout is ABI-dependent.

### Q5: Sret in emitter - how to handle Ret with multiple values?

**Context:** For sret, VInst::Ret with N values needs to store them to the buffer pointed to by s1, not return in registers.

**Current VInst::Ret:**
```rust
Ret { vals: Vec<VReg> }  // up to 4 for direct, up to 16 for sret
```

**Options:**
- Emitter checks `FuncAbi::is_sret()` and switches strategy
- Lowering already produces different code for sret (but lower doesn't know ABI)
- Add Ret variant for sret (unnecessary complexity)

**My suggestion:** Emitter checks FuncAbi - it has all the info needed.

## Open Questions

None after above decisions.
