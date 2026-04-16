# Phase 2: Simple Calls (Allocator Core)

## Scope

Implement the 4-step call handling algorithm in the backward walk. Add verifier
checks. Validate with LPIR filetests and parametric builder tests.

## Implementation

### 1. Call detection in walk.rs

Add a `VInst::is_call()` check (already exists) in the backward walk loop. When
a call is detected, branch to dedicated call processing instead of generic
def/use handling.

### 2. Step 1 — Defs (return values)

For each ret vreg at index `i` (via `VInst::Call.rets`):

- Target = `RET_REGS[i]` (a0, a1)
- Record `allocs[def_idx] = Alloc::Reg(target)`
- If vreg in pool: emit `Move(Reg(target) → Reg(pool_reg))` at `After(call)`,
  free from pool
- If vreg spilled: emit `Move(Reg(target) → Stack(slot))` at `After(call)`
- If vreg dead (`Alloc::None`): no move needed

### 3. Step 2 — Relocate precolored a-reg vregs

Scan `pool.iter_all_occupied()` for vregs in a-regs (x10-x17). For each:

- Allocate a pool reg via `pool.alloc(vreg)` (prefer s-regs if we add that
  optimization later)
- Move vreg from a-reg to pool reg: `pool.free(a_reg)`, vreg now in pool reg
- The entry-move logic at walk end will see the param in its pool reg and emit
  `Move(a_reg → pool_reg)` at `Before(0)`

Note: this step only matters when entry params are still in a-regs. After the
first call in a function, params have already been relocated.

### 4. Step 3 — Clobber save/restore

Identify clobbered pool regs: t-regs in `ALLOC_POOL` that are occupied.
Specifically: `{t0(5), t1(6), t2(7), t4(29), t5(30), t6(31)}`.

For each occupied t-reg:

- Get/assign spill slot: `spill.get_or_assign(vreg)`
- Emit `Move(Reg(t_reg) → Stack(slot))` at `Before(call)` — save
- Emit `Move(Stack(slot) → Reg(t_reg))` at `After(call)` — restore
- **Keep vreg in pool** (don't evict) — transparent to backward walk

### 5. Step 4 — Uses (arguments)

For each arg vreg at index `i` (via `VInst::Call.args`):

- If `i >= 8`: skip fixed-reg assignment, process as normal use (emitter
  handles stack args)
- Target = `ARG_REGS[base + i]` where `base = 0` (sret shift is Phase 3)
- If vreg in pool at `preg`:
  - If `preg == target`: no move, record `Alloc::Reg(target)`
  - Else: emit `Move(Reg(preg) → Reg(target))` at `Before(call)`, record
    `Alloc::Reg(target)`
- If vreg spilled: emit `Move(Stack(slot) → Reg(target))` at `Before(call)`,
  record `Alloc::Reg(target)`
- If vreg not allocated: `pool.alloc(vreg)`, then move to target as above

Vreg stays in its pool reg for the backward walk (the arg move is a copy).

### 6. Edit ordering

Edits at `Before(call)` must be ordered: saves first, then arg moves. This
ensures arg moves read from correct register values (not yet overwritten by
other arg setups).

Edits at `After(call)` are: ret moves first, then restores.

### 7. Verifier: call-specific checks

In `verify.rs`, add `verify_call_abi`:

- For each `VInst::Call`: def operand `i` must be `Alloc::Reg(RET_REGS[i])`
- For each `VInst::Call`: use operand `i` (where `i < 8`) must be
  `Alloc::Reg(ARG_REGS[i])`

### 8. LPIR filetests

Create `filetests/call/` with:

| File | LPIR pattern | Key assertion |
|------|-------------|---------------|
| `simple.lpir` | `v2 = call @f(v0, v1); ret v2` | arg→a0/a1, ret→a0, moves |
| `live_across.lpir` | `v1 = iconst; v2 = call @f(v0); v3 = add v1, v2; ret v3` | v1 save/restore around call |
| `arg_reuse.lpir` | `v2 = call @f(v0, v1); v3 = add v1, v2; ret v3` | v1 is arg AND live after |
| `chain.lpir` | `v2 = call @f(v0, v1); v3 = call @g(v0, v2); ret v3` | ret→arg flow |
| `multi_live.lpir` | 3 iconsts + call + use all 3 after | multiple save/restores |
| `callee_saved.lpir` | `pool_size: 2` forcing s-regs, value in s-reg at call | no save needed for s-reg |

### 9. Builder parametric tests

In `fa_alloc/test/builder.rs`:

- `call_with_live_value(pool: 1,2,4,8,16)` — iconst, call, use iconst after.
  Assert: verifier passes, spill ≥ 1 at small pools.
- `call_chain(pool: 1,2,4,8)` — ret of call A → arg of call B. Assert:
  structural correctness.
- `multi_arg_call(pool: 1,2,4)` — call with 4-6 args. Assert: verifier passes.

## Validation

```bash
# Allocator unit tests
cargo test -p lpvm-native fa_alloc

# LPIR filetests
cargo test -p lpvm-native --test filetests

# Verify all existing tests still pass
cargo test -p lpvm-native
```

## Success Criteria

- All 6 call LPIR filetests pass (BLESS then verify)
- Builder parametric tests pass across all pool sizes
- Verifier catches incorrect ARG_REG / RET_REG assignments
- No regressions in existing spill/param filetests
