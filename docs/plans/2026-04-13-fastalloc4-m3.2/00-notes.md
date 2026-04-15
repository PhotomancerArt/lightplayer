# M3.2: Calls + Sret - Analysis Notes

## Scope

Extend the allocator to handle function calls:
1. **Simple calls** - args in ARG_REGS, return in RET_REGS, clobber handling
2. **Sret calls** - callee returns via pointer to stack buffer
3. **Stack-passed arguments** - outgoing args beyond a0-a7
4. **Verifier** - structural checks that allocation respects call ABI
5. **Tests** - filetests and parametric tests for each scenario

## Current State

### What works (from M2 + M3.1)

- `walk_linear()` backward walk allocator for straight-line code
- Entry param precoloring (params seeded at ABI regs)
- Entry moves recorded when params displaced (triggers with call clobbers)
- Spill/reload logic with LRU eviction
- Filetest infrastructure: 11 `.lpir` files with BLESS mode, trace interleaving
- `AllocOutput` with per-operand allocs and edit list
- Frame layout with sret buffer support (`abi/frame.rs`)
- `verify_alloc` checks structural invariants after allocation
- `EditPoint::After(u16)` defined and ordered, but unused so far

### What exists but is stubbed/unused

- **`walk.rs`**: Treats `VInst::Call` generically via `for_each_def` (rets) and
  `for_each_use` (args). No call clobber handling. `func_abi.call_clobbers()` is
  never called.
- **`emit_vinst`**: Complete stub — no instruction emission.
- **`callee_uses_sret`**: Set by lowerer on `VInst::Call`, unused by walk/emit.
- **`caller_saved_int()`**: Returns `PregSet` of a0-a7, t0-t6. Stored in
  `FuncAbi.caller_saved`, exposed via `call_clobbers()`. Never consulted.
- **Frame sret buffer**: `FrameLayout::compute` accepts `caller_sret_bytes` but
  `emit_lowered` passes `0`. `ModuleAbi` tracks `max_callee_sret_bytes`.

### Key data structures

- `VInst::Call { target, args, rets, callee_uses_sret, src_op }` — args/rets are
  `VRegSlice` into vreg_pool
- `FuncAbi::call_clobbers()` → `PregSet` (a0-a7, t0-t6)
- `ReturnMethod::Sret { ptr_reg: A0, preserved_reg: S1, word_count }`
- `SRET_SCALAR_THRESHOLD = 2` (>2 scalar return words → sret)
- `ALLOC_POOL = [t0,t1,t2,t4,t5,t6,s2..s11]` (16 regs, excludes a0-a7)
- `ARG_REGS = [a0..a7]`, `RET_REGS = [a0,a1]`

---

## Resolved Questions

### Q1: Call handling algorithm ✓

**Decision:** Four-step processing when the backward walk hits `VInst::Call`:

**Step 1 — Defs (return values):** For each ret vreg at index i, constrain to
`RET_REGS[i]`. Record `Alloc::Reg(RET_REG)`. If vreg was in a pool reg, emit
`Move(RET_REG → pool_reg)` at After(call). Free from pool. If spilled, emit
`Move(RET_REG → Stack(slot))` at After(call). Doing defs first avoids saving
ret vregs that are about to be freed.

**Step 2 — Relocate precolored a-reg vregs:** Any entry param still sitting in
an a-reg must be moved to a pool reg before clobber handling. Prefer s-regs
(callee-saved, survive calls). This integrates with existing entry-move logic:
after the walk, entry moves emit `Move(a_reg → pool_reg)` at Before(0).

**Step 3 — Clobber save/restore:** For each live vreg in a pool t-reg
(caller-saved), insert `Move(t_reg → Stack(slot))` at Before(call) and
`Move(Stack(slot) → t_reg)` at After(call). **Keep the vreg in the pool** (don't
evict). This differs from regalloc2 which evicts to stack and relies on def-site
spills — our approach keeps values in registers for pre-call code, more efficient
for linear regions. The save/restore pair is transparent to the backward walk.

**Step 4 — Uses (arguments):** For each arg vreg at index i, ensure it reaches
`ARG_REGS[i]`. If in pool, emit `Move(pool_reg → ARG_REG)` at Before(call).
Record `Alloc::Reg(ARG_REG)`. The vreg stays in its pool reg for the backward
walk (the move is a copy). vmctx as arg0 typically needs no move if entry
relocation placed it in a pool reg and the move to a0 is just the arg setup.

**Edit ordering at Before(call):** Saves first, then arg moves. Ensures arg
moves read from correct register values.

**Cross-reference with regalloc2:** regalloc2 fastalloc does: reserve fixed
regs → remove clobbers from availability → evict from fixed regs → evict from
clobber regs → allocate late ops (defs) → allocate early ops (uses). Key
differences: (1) regalloc2 evicts clobbered vregs from pool, creating pessimistic
def-site spills; we keep in pool with explicit save/restore. (2) regalloc2
handles clobbers before defs; we handle defs first to avoid saving ret vregs
about to be freed. (3) regalloc2 doesn't have our a-reg precolor concept.

### Q2: Clobber scope ✓

**Decision:** Clobber handling covers two sets:

1. **Pool t-regs** `{t0,t1,t2,t4,t5,t6}` — standard save/restore
2. **Precolored a-regs** — entry params still in a-regs are relocated to pool
   regs (step 2 above), then standard clobber rules apply to wherever they end up

S-regs in pool survive calls (callee-saved), no action needed.

### Q7/Q8: Test coverage ✓

**LPIR filetests** (allocator-level, `lpvm-native/filetests/call/`):

| File | Edge case | Validates |
|------|-----------|-----------|
| `call/simple.lpir` | 1 arg + vmctx, 1 ret | arg→ARG_REG, ret→RET_REG moves |
| `call/live_across.lpir` | Value live before and after call | Clobber save/restore for t-reg |
| `call/arg_reuse.lpir` | Value is both arg AND live after | Save before arg setup, restore after |
| `call/chain.lpir` | Ret of call A → arg of call B | Return value flows to next arg |
| `call/multi_live.lpir` | 3+ values live across call | Multiple save/restore pairs |
| `call/callee_saved.lpir` | Value in s-reg at call time | No save/restore needed |
| `call/sret_simple.lpir` | `callee_uses_sret` call | a0=sret ptr, args shifted |
| `call/stack_args.lpir` | >8 args to call | Stack-passed argument handling |

**GLSL filetests** (execution-level, `lps-filetests/filetests/lpvm/native/`):

Existing coverage is comprehensive (18 files covering simple calls, multi-args,
nested, sret vec4/mat4, stack args, control flow, caller-save pressure). Gaps to
add alongside existing files:

- **Arg live after call**: variable used as call arg AND after the call
  (`foo(x); return x + result`)
- **Sret call chain**: result of sret call used as arg to another call
- **Sret + stack args**: callee with sret AND >7 user args (overflow at 7 not 8)

**Verifier checks to add:**
- Ret operands in correct `RET_REGS[i]`
- Arg operands in correct `ARG_REGS[i]`
- No live vreg in caller-saved pool reg survives call without save/restore edits
- Sret calls: arg indices shifted correctly

---

## Open Questions

### Q3: Sret scope ✓

**Decision:** Both caller-side and callee-side, as separate phases. **Callee-side
first** because:

- The filetest harness is the caller — it can call our sret-returning function
  directly and verify the result.
- Caller-side sret requires a working callee (someone must write to the buffer).
- Callee-side is mostly emitter work: prologue saves `a0→s1`, `Ret` stores
  return values to `[s1]` buffer. `s1` already excluded from `ALLOC_POOL` when
  `is_sret`. Allocator impact is small.

Phase order:
1. **Callee-side sret**: our function returns >2 words via sret buffer. Emitter
   focus (prologue `a0→s1`, ret stores). Testable immediately via GLSL filetests.
2. **Caller-side sret**: our function calls a callee that returns sret. Allocator
   focus (set up `a0` as sret ptr, arg shift, read results from buffer after).
   Requires callee-side to be working first.

### Q4: Sret arg shift ✓

**Decision:** Derive on the fly from `callee_uses_sret` flag + arg count on
`VInst::Call`. No separate `CallAbi` struct needed.

Mapping: `args[i] → ARG_REGS[base + i]` where `base = 1` if `callee_uses_sret`,
`0` otherwise. The `args` slice contains `[vmctx, user_arg0, ...]` — the sret
buffer pointer is NOT in the args slice (it's a frame-relative SP offset, set up
by the emitter, not a vreg).

For sret calls, the emitter handles: `a0 = SP + sret_slot_base_from_sp` (an
`addi` instruction before the call). The allocator just shifts arg register
assignments by 1.

### Q5: Stack-passed arguments ✓

**Decision:** Emitter responsibility. The allocator processes `args[0..8]` as
register-constrained (→ `ARG_REGS`). `args[8..]` are processed as normal uses —
the vreg just needs to be in a pool reg or spill slot. The emitter knows which
args are stack-passed (index ≥ 8 for non-sret, ≥ 7 for sret) and emits `sw` to
the outgoing stack area at the correct SP offsets.

Cross-ref with regalloc2: backends mark overflow args as `Stack` constraint or
`FixedReg(fixed_stack_preg)`. The allocator assigns spill slots but the frame
layout / stores are the backend's problem. Same principle — our approach is
simpler since we don't need per-operand constraints.

### Q6: Verifier checks for calls ✓

**Decision:** Add two structural checks now, defer liveness-aware check:

1. **Ret operands in correct RET_REGS**: For `VInst::Call`, def operand `i` must
   be `Alloc::Reg(RET_REGS[i])`.
2. **Arg operands in correct ARG_REGS**: For `VInst::Call`, use operand `i`
   (where `i < 8`) must be `Alloc::Reg(ARG_REGS[base + i])` where `base = 1` if
   `callee_uses_sret`, else `0`.

Deferred to future work (see `docs/design/native/future-work.md`):
3. **Clobber safety**: verify every vreg live in a caller-saved pool reg across a
   call has a matching save/restore edit pair. Requires per-instruction liveness
   tracking in the verifier. GLSL execution filetests cover this in the meantime.

### Q9: Parametric builder tests ✓

**Decision:** Three parametric tests sweeping `pool_size`, using structural
assertions (verify_alloc + spill count bounds, no snapshot strings):

- `call_with_live_value(pool: 1,2,4,8,16)` — one value live across a call.
  Assert: no panic, spill ≥ 1 at small pools, verifier passes.
- `call_chain(pool: 1,2,4,8)` — return of call A → arg of call B. Assert:
  structural correctness, spill count scales with pressure.
- `multi_arg_call(pool: 1,2,4)` — call with 4-6 args. Assert: no panic, arg
  regs correct (via verifier).

### Q10: Filetest directives and builder capabilities ✓

**Decision:** Go straight to explicit import directives (full control from the
start, avoids Q32-only limitations).

**Filetest directive:** `; import: name(i32, i32) -> vec4` declares a callee
import. The filetest parser sets up the import in the LPIR module so
`LpirOp::Call` can reference it. Gives full control: any arg count, any return
type (scalar or sret), stack args.

**Builder:** Add `.call(target, args, rets, callee_uses_sret)` method to emit
`VInst::Call` directly. Add `abi_return` with type info for callee-side sret
testing.
