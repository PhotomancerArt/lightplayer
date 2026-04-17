# Phase 2 — Inline scaffold + `recompute_offsets`

## Scope of phase

Stand up the empty **`lpir::inline`** module with the public API stubs
(returning **`InlineResult::default()`**) and the first real piece of
machinery: **`recompute_offsets(&mut [LpirOp])`**. The orchestration
loop, callgraph, splicer, and heuristic come in later phases.

`recompute_offsets` is the foundational reusable utility — it walks a
mutated body, matches structural markers to their openers via a stack,
and patches **`else_offset`** / **`end_offset`** /
**`continuing_offset`** in place. Once Phase 1's **`Continuing`** marker
exists, every offset on every opener is recoverable purely from
markers.

## Code Organization Reminders

- New submodule layout per Q2 in **`00-design.md`**:
  - `lpir/src/inline/mod.rs`
  - `lpir/src/inline/offsets.rs`
- Re-export only the public surface from **`lpir/src/lib.rs`**:
  **`inline_module`**, **`InlineResult`**. Internal helpers stay
  crate-private.
- One concept per file; **`offsets.rs`** is just the recompute helper
  and its tests-of-record (full coverage lives in
  `tests/inline_offsets.rs`).

## Implementation Details

### `lpir/src/inline/mod.rs`

```rust
//! LPIR inlining pass — bottom-up, never deletes functions, structural
//! offset recompute. See docs/plans/2026-04-17-lpir-inliner-stage-iii.

mod offsets;

pub(crate) use offsets::recompute_offsets;

#[derive(Debug, Default, Clone, Copy)]
pub struct InlineResult {
    pub functions_inlined: usize,
    pub call_sites_replaced: usize,
    pub functions_skipped_recursive: usize,
    pub budget_exceeded: bool,
}

pub fn inline_module(
    _module: &mut crate::LpirModule,
    _config: &crate::InlineConfig,
) -> InlineResult {
    // Filled in by Phase 6.
    InlineResult::default()
}
```

### `lpir/src/inline/offsets.rs`

- **`pub(crate) fn recompute_offsets(body: &mut [LpirOp])`**.
- Walk forward over **`body`**. Maintain a stack of **`(opener_idx,
  Opener)`** entries where **`Opener`** is a small internal enum
  capturing which opener variant we're inside (**`If`** / **`Loop`** /
  **`Switch`** / **`Block`**).
- On **`Else`**: peek top, must be **`If`**, patch
  **`body[opener_idx].as_if_mut().else_offset = current_idx`** (or
  whatever the existing field name is — match the struct shape exactly).
- On **`Continuing`**: peek top, must be **`Loop`**, patch
  **`continuing_offset = current_idx`**.
- On **`End`** / **`ExitBlock`**: pop. For the matching opener, patch
  **`end_offset = current_idx`** (or **`exit_offset`** for **`Block`** —
  match existing field names).
- On any opener: push **`(current_idx, kind)`**. Inner offsets are
  patched by inner pops first, so an outer recompute is correct as long
  as we patch on the way *up* (i.e. when we see the marker, not when
  we push).
- Debug-assert the stack is empty at end-of-body.

This function never reads existing offset values — it always overwrites
from the markers. That makes it idempotent and order-independent within
a single call.

### `lpir/src/lib.rs`

- **`pub mod inline;`** (or `mod inline;` + targeted `pub use`).
- **`pub use inline::{inline_module, InlineResult};`**.

## Tests (`lpir` crate)

`tests/inline_offsets.rs` (new):

- **`if_else_end`**: build via **`FunctionBuilder`**, then *zero out*
  every offset field, call **`recompute_offsets`**, assert they match
  the original.
- **`loop_with_continuing_marker`**: same, including a **`Continuing`**
  marker midway through the body.
- **`loop_without_continuing_marker`**: legacy form (no marker) — the
  recomputed **`continuing_offset`** should equal **`loop_start_pc + 1`**
  (i.e. unchanged from the legacy convention; verify the helper handles
  this either by leaving the existing offset alone or by patching to the
  same value).
- **`switch_multi_arm`**: nested case if **`SwitchStart`** carries
  per-arm offsets — match whatever shape exists today.
- **`block_exit`**: one **`Block`** + **`ExitBlock`**; assert
  **`end_offset`** patched.
- **`nested_loop_in_if_in_block`**: stress nesting; offsets must all
  match a fresh build of the same structure.
- **`mutated_body_grows`**: take a built body, splice in extra
  no-op-ish ops between an opener and its closer, run
  **`recompute_offsets`**, assert offsets shifted correctly.

## Validate

```bash
cargo test -p lpir
```

The scaffold's stub **`inline_module`** is a no-op; nothing else in the
workspace can depend on it yet, so only the **`lpir`** crate needs to
build/test in this phase.
