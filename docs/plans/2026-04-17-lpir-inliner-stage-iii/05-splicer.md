# Phase 5 — Body splicer

## Scope of phase

Add **`lpir/src/inline/splice.rs`**: the function that actually replaces
one **`LpirOp::Call`** in a caller with the cloned, remapped body of a
callee. This is where the **return-shape analysis** from `00-design.md`
lives, and it's the only place that mutates `caller.body` for inlining.

Per Q14, the splicer is **mutative on the caller** — it does not
allocate a parallel `Vec<IrFunction>`. Memory for the call-site `Call`
op is reclaimed by `Vec::splice`.

The orchestration loop that *calls* this for every site comes in
Phase 6; tests in this phase exercise the splicer directly.

## Code Organization Reminders

- One file: `lpir/src/inline/splice.rs`. Crate-private.
- `inline_call_site` is the only public-to-`inline` function.
- All offset patching is deferred to a single
  **`recompute_offsets(&mut caller.body)`** call by the orchestrator
  after *all* of a caller's sites are spliced (Phase 6). The splicer
  itself never touches offsets.

## Implementation Details

### Signature

```rust
pub(crate) fn inline_call_site(
    caller: &mut IrFunction,
    callee: &IrFunction,
    call_op_idx: usize,
);
```

The caller, callee, and call-site index are picked by Phase 6. The
function must not panic on any well-formed input.

### Step 1 — Read & destructure the call site

- Snapshot the **`Call`** op: extract **`args: VRegRange`** and
  **`results: VRegRange`**, resolve to **`Vec<VReg>`** via
  `caller.vreg_pool`.
- Validate against callee shape: `args.len() == 1 +
  callee.param_count` (the `+1` is vmctx); `results.len() ==
  callee.return_count`. Debug-assert; in release, log and bail (return
  without splicing) — the orchestrator counts this as "not inlined".

### Step 2 — Param-write scan + remap

```rust
let pw   = scan_param_writes(callee);
let rmap = build_remap(caller, callee, &call_args, &call_results, &pw);
```

### Step 3 — Return-shape analysis

Walk **`callee.body`** once and classify:

```rust
enum ReturnShape {
    /// Zero `Return` ops (unreachable terminator) OR void return.
    None,
    /// Exactly one `Return` and it's the very last op of callee.body.
    SingleAtEnd,
    /// Anything else: multiple Returns, or a Return not at the end.
    Multi,
}
```

This decides how `Return` ops are rewritten and whether the inlined
body needs a `Block { … } / ExitBlock` wrapper.

### Step 4 — Build the scratch `Vec<LpirOp>`

In order:

1. **Param copies**: extend with `rmap.param_copies` (already in
   correct form, vregs already in caller-space).
2. **`Block` opener** (only if `ReturnShape::Multi`):
   `LpirOp::Block { end_offset: 0 }` — placeholder offset, fixed by
   `recompute_offsets`.
3. **Cloned + remapped body**: walk `callee.body` op by op:
   - If op is **`LpirOp::Return { values }`**:
     - Resolve each return value vreg through `rmap.vreg_table`.
     - Emit `LpirOp::Copy { dst: call_results[k], src: remapped }` for
       each `k` (or whatever the multi-return primitive is — match
       existing return-handling lowering; if a single move-list op
       exists, use that instead of N `Copy` ops).
     - If `ReturnShape::Multi`: append `LpirOp::ExitBlock`.
     - If `ReturnShape::SingleAtEnd`: no `ExitBlock` needed; this is
       the last op anyway.
     - If `ReturnShape::None`: no Returns to rewrite — but if we hit
       one, classification was wrong → debug-assert.
   - Else: push `remap_op(op, &rmap, &mut caller.vreg_pool,
     &callee.vreg_pool)`.
4. **`ExitBlock` close** (only if `ReturnShape::Multi`): append one
   final `LpirOp::ExitBlock` to terminate the wrapper if the last
   callee op was *not* a Return (otherwise step 3 already emitted it).
   - Cleaner formulation: track `last_was_exit_block: bool` while
     building; emit a trailing `ExitBlock` iff
     `Multi && !last_was_exit_block`.

### Step 5 — Splice into caller

```rust
caller.body.splice(call_op_idx..=call_op_idx, scratch);
```

Single splice replaces the `Call` op in place. Capacity reclamation is
implicit; for embedded targets we may want a follow-up
`caller.body.shrink_to_fit()` once per caller after all sites are done
(Phase 6 calls it once at the end).

### Step 6 — Slot/vreg counts

After splice, ensure:

- `caller.slot_count` already incremented by `build_remap`.
- `caller.vreg_count` reflects fresh allocations made by `build_remap`.

The splicer doesn't touch these directly — they were updated when
`build_remap` allocated.

### What the splicer does *not* do

- Does **not** call `recompute_offsets`. Phase 6 batches that per
  caller after all sites are processed (avoids `O(sites × body_len)`
  re-walks).
- Does **not** validate the result. Phase 6's orchestrator runs
  validation in debug builds.
- Does **not** delete the callee. Per Q14, dead-function elimination is
  M5.

## Tests (`lpir` crate)

`tests/inline_basic.rs` (new): drive `inline_call_site` directly with
hand-built modules; after each splice, run **`recompute_offsets`** then
**`validate`** then **`interp::run_function`** (or whatever the
existing test harness uses) and compare results with the same module
*pre*-inlining.

- **`void_callee`**: callee returns nothing, single statement body
  (e.g. write to a slot). Result: same observable side effect, no
  result vreg writes.
- **`single_return_at_end`**: `fn add1(a) -> a + 1`. Inlining produces
  no `Block`, no `ExitBlock`. Verify caller body shape and result.
- **`single_return_not_at_end`**: callee with an early `Return` inside
  an `If`. Should classify as `Multi`, wrap in `Block`/`ExitBlock`.
- **`multiple_returns`**: callee with two `Return`s in different `If`
  arms. Wrapped in `Block`; both Returns become `Copy + ExitBlock`.
- **`nested_call_in_callee`**: callee body itself contains a `Call`
  to an import. Verify `vreg_pool` splice happens correctly via
  `remap_op` and the inlined call still references the right import.
- **`mutated_param`**: callee writes to its first param. Verify a
  `Copy` is emitted into a fresh vreg and subsequent reads use that.
- **`readonly_param`**: callee never writes its params. Verify zero
  `Copy` ops, direct alias.
- **`vmctx_propagation`**: any callee op that reads `VReg(0)` (vmctx)
  remains reading `VReg(0)` post-splice.
- **`slot_remap`**: callee uses 2 slots; caller has 3 pre-inlining.
  Post-inlining, callee's slot uses are at 3, 4.

For each test: build module via `FunctionBuilder`, snapshot expected
behavior via `interp::run_function`, splice, recompute offsets,
validate, re-interp, compare.

## Validate

```bash
cargo test -p lpir
```
