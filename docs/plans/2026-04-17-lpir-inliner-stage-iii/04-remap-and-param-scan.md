# Phase 4 — Remap helpers + param-write scan

## Scope of phase

Add **`lpir/src/inline/remap.rs`**: the per-call-site machinery that
prepares the callee body for splicing into a caller. Three pieces:

1. **`scan_param_writes(callee) -> ParamWriteMask`** — which params are
   written by the callee body (for the per-param alias-or-copy
   strategy from Q4 in `00-design.md`).
2. **`build_remap(...)`** — produce the **`VReg`** translation table
   plus the list of preamble **`Copy`** ops needed for mutated params.
3. **`remap_op(...)`** — clone a single callee op with **`VReg`** /
   **`SlotId`** / **`vreg_pool`** fixups applied.

Splicing itself (Phase 5) drives these helpers; this phase tests them
in isolation.

## Code Organization Reminders

- One file: `lpir/src/inline/remap.rs`. Crate-private.
- Keep helpers pure: no module mutation here. `build_remap` and
  `remap_op` produce data; the splicer (Phase 5) applies it.
- Use **`alloc::vec::Vec`** indexed by callee **`VReg::index()`** for
  the translation table — dense, **`O(1)`** lookup, deterministic.

## Implementation Details

### `ParamWriteMask`

```rust
/// Bit per callee param (excluding vmctx). True = param is written
/// somewhere in the callee body (definitely a `Copy` is needed).
pub(crate) struct ParamWriteMask {
    /// One bool per param in callee param order (params live in
    /// VReg(1)..=VReg(param_count); index 0 here = first non-vmctx).
    pub written: Vec<bool>,
}

pub(crate) fn scan_param_writes(callee: &IrFunction) -> ParamWriteMask;
```

- Iterate **`callee.body`**. For each op, ask
  **`op.def_vreg() -> Option<VReg>`** (already exists in
  **`lpir_op.rs`**).
- If the defined **`VReg`** falls in the param range
  (**`1..=callee.param_count`**), mark
  **`written[idx_of(vreg)] = true`**.
- Skip **`VReg(0)`** — vmctx is read-only by construction; debug-assert
  it never appears as **`def_vreg`**.

### `build_remap`

```rust
pub(crate) struct Remap {
    /// callee VReg index → caller VReg.
    pub vreg_table: Vec<VReg>,
    /// Preamble `Copy` ops (param mutated → fresh caller vreg from arg).
    /// Empty for read-only params (those alias arg vreg directly).
    pub param_copies: Vec<LpirOp>,
    /// Slot offset to add to callee SlotId references.
    pub slot_offset: u32,
}

pub(crate) fn build_remap(
    caller: &mut IrFunction,
    callee: &IrFunction,
    call_args: &[VReg],          // resolved from call site's VRegRange
    call_results: &[VReg],       // resolved from call site's result range
    param_writes: &ParamWriteMask,
) -> Remap;
```

- Allocate **`vreg_table`** sized to **`callee.vreg_count`**. Initialize
  to a sentinel (e.g. **`VReg::INVALID`** or `VReg::from_index(u32::MAX)`).
- **`vreg_table[0] = VMCTX_VREG`** — vmctx always aliases.
- For each param **`i`** in `1..=callee.param_count`:
  - Caller's arg vreg for that param is `call_args[i]` (call_args[0] is
    vmctx, by Q4 convention; verify against existing call lowering).
  - If **`!param_writes.written[i-1]`**: alias —
    `vreg_table[i] = call_args[i]`.
  - Else: allocate fresh `caller.alloc_vreg()` → `vreg_table[i] = new`,
    push **`LpirOp::Copy { dst: new, src: call_args[i] }`** into
    `param_copies`.
- For each non-param vreg **`v`** in
  `callee.param_count+1..callee.vreg_count`:
  - If `v` is one of the callee's return vregs **and** the corresponding
    `call_results[k]` slot exists, alias to that result vreg (Phase 5
    rewrites Returns to write directly there). Otherwise allocate fresh.
  - For now (this phase), allocate fresh for *all* non-params; the
    Return-to-result aliasing is decided by Phase 5's return-shape
    analysis based on the actual `Return` operand list. Keep
    `build_remap` shape-agnostic.
- **`slot_offset = caller.slot_count`**; reserve `callee.slot_count`
  fresh slots in caller (call `caller.alloc_slot()` in a loop, or bump
  the count directly — match the existing API in `IrFunction`).

Debug-assert: every entry in `vreg_table` is non-sentinel before
returning.

### `remap_op`

```rust
pub(crate) fn remap_op(
    op: &LpirOp,
    remap: &Remap,
    caller_vreg_pool: &mut Vec<VReg>,
    callee_vreg_pool: &[VReg],
) -> LpirOp;
```

- Clone **`op`**, then for each **`VReg`** field replace with
  **`remap.vreg_table[v.index()]`**.
- For each **`SlotId`** field, add **`remap.slot_offset`**.
- For any **`VRegRange`** that indexes into the callee's `vreg_pool`
  (e.g. **`Call { args, results }`** for nested calls inside the
  callee body): read the slice from `callee_vreg_pool`, remap each
  vreg through `vreg_table`, append to `caller_vreg_pool`, and rewrite
  the `VRegRange` to point at the new caller-pool location.
- Markers (**`Else`** / **`End`** / **`ExitBlock`** / **`Continuing`**)
  and openers' offset fields: leave offsets at zero / placeholder.
  Phase 5 splices the body, Phase 6's
  **`recompute_offsets`** call (after splice) fixes them.
- Don't touch **`Return`** here — Phase 5's splicer handles return
  rewriting before calling `remap_op` (or skips Returns entirely and
  emits the rewritten form directly).

## Tests (`lpir` crate)

`tests/inline_param_writes.rs` (new):

- **`vmctx_never_written`**: assert via debug-build test that scanning
  any well-formed callee never marks `VReg(0)` as written; trivial
  callees produce all-false masks.
- **`single_param_read_only`**: callee `fn(a) -> a + 1` → mask
  `[false]`.
- **`single_param_mutated`**: callee where `a` is the dst of an `Add`
  → mask `[true]`.
- **`multi_param_mixed`**: 3 params, second one mutated → `[false,
  true, false]`.

`tests/inline_remap.rs` (new):

- **`alias_for_readonly_param`**: `build_remap` produces empty
  `param_copies` and aliases vreg directly.
- **`copy_for_mutated_param`**: `param_copies` length 1, fresh dst
  vreg, src is caller arg vreg.
- **`vmctx_aliases`**: `vreg_table[0] == VMCTX_VREG` regardless of
  param-write mask.
- **`slot_offset_applied`**: callee with 2 slots inlined into caller
  with 3 slots → remapped slot ids are 3 and 4.
- **`vreg_pool_splice`**: callee body contains a `Call` to an import
  with multiple args; after `remap_op`, caller's `vreg_pool` has the
  spliced entries with translated vregs and the new `Call`'s
  `VRegRange` points at them.

## Validate

```bash
cargo test -p lpir
```
