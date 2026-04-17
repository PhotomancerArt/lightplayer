# Phase 1 — `LpirOp::Continuing` marker (M2.5)

## Scope of phase

Add **`LpirOp::Continuing`** as a structural marker for the start of a
loop's continuing block, mirroring how **`Else`** marks the start of an
if's else arm. The cached **`LoopStart::continuing_offset`** field is
**kept** — backends and the interpreter keep using it unchanged. The
marker exists purely so structural passes (Phase 2's
**`recompute_offsets`**) can rebuild the cache after body mutation.

This phase is the M2.5 prerequisite from
`docs/roadmaps/2026-04-15-lpir-inliner/m2.5-continuing-marker.md`,
folded in as the first phase of stage III.

## Code Organization Reminders

- One concept per change: a single new variant + one no-op arm per
  consumer. No drive-by refactoring of nearby code.
- Backend changes are minimal — every consumer just needs to *not panic*
  on the new variant. Don't restructure existing match logic.
- Keep **`#![no_std]`** + **`alloc`** — no new heap usage required.

## Implementation Details

### `lpir/src/lpir_op.rs`

- Add **`Continuing`** variant to **`LpirOp`** enum (no fields).
- Update **`LpirOp::def_vreg(&self)`** to return **`None`** for it
  (matches other markers like **`Else`** / **`End`**).

### `lpir/src/builder.rs`

- **`FunctionBuilder::push_continuing()`**: prepend
  **`self.body.push(LpirOp::Continuing)`** before the existing
  **`continuing_offset`** patch on the open **`LoopStart`**. The patched
  offset must equal the index of the just-pushed **`Continuing`** op.

### `lpir/src/parse.rs`

- The existing **`continuing:`** text token already triggers the
  **`continuing_offset`** patch. Update that path to also call
  **`fb.push_continuing()`** so the marker lands in the body.

### `lpir/src/print.rs`

- Add a match arm for **`LpirOp::Continuing`** that prints
  **`continuing:`** (no trailing brace, like **`else:`**).
- Remove the existing logic that conditionally prints **`continuing:`**
  based on whether **`continuing_offset != start_pc + 1`**. The marker
  is now the single source of truth for placement; just print it where
  it appears in the body.

### `lpir/src/validate.rs`

- Add **`Continuing`** arms to all exhaustive matches that mention
  **`Else`** / **`End`** / opener variants.
- Structural check: **`Continuing`** is only legal inside a
  **`LoopStart`** … **`End`** pair, and not nested inside another
  **`IfStart`** / **`SwitchStart`** / **`Block`** / inner **`LoopStart`**
  inside that loop. Reuse the existing control-flow stack walk; on
  encountering **`Continuing`**, assert the top of the stack is the
  expected **`LoopStart`**.
- Validate **`LoopStart::continuing_offset`** points at a **`Continuing`**
  op when present, **or** at **`start_pc + 1`** if no marker is in the
  body (legacy behavior — keep both legal).

### `lpir/src/interp.rs`

- One arm in the dispatch loop:
  **`LpirOp::Continuing => { pc += 1; }`**.

### `lpir/src/const_fold.rs`

- Add **`| LpirOp::Continuing`** to the conservative-clear arm next to
  the other markers (**`Else`** / **`End`** / opener variants), so
  constant propagation state is reset across the boundary, matching how
  control-flow joins are handled today.

### `lpvm-native/src/lower.rs`

- One match arm: **`LpirOp::Continuing => { /* structural marker, no
  emit */ }`**. The existing range-based continuing-block lowering
  already starts at **`continuing_offset`** which now points at the
  marker, so the marker is naturally inside the lowered slice and the
  no-op arm makes it skip cleanly.

### `lpvm-wasm/src/emit/ops.rs`

- One match arm: same no-op pattern as native.

### `lpvm-cranelift/src/emit/control.rs`

- One match arm: same no-op pattern as native.

## Tests (`lpir` crate)

Extend existing test files; do not add a new module just for this.

- `tests/all_ops_roundtrip.rs`: add a loop with an explicit
  **`continuing:`** body to the round-trip set.
- `tests/block_ops.rs` (or wherever loop validation tests live —
  inspect first; create a small new file only if no good home exists):
  one test asserting that after `parse → build`, the
  **`LoopStart::continuing_offset`** value equals the index of the
  **`Continuing`** op in the body.

## Validate

```bash
cargo test -p lpir
cargo test -p lpvm-native
cargo test -p lpvm-wasm
cargo test -p lpvm-cranelift
cargo test -p lps-filetests -- --test-threads=4
```

No behavioral change is expected — every existing test must pass
unchanged. The marker is purely additive.
