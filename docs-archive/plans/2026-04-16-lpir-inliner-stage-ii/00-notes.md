# Plan notes: LPIR inliner M2 — Block / ExitBlock

## Scope of work

Implement structured forward-only regions in LPIR (`Block { end_offset }`, `ExitBlock`) so multi-return inlined callees can merge at one point without misusing `LoopStart` + `Break`. The matching closer is the existing **`End`** marker (same convention as `IfStart` / `LoopStart` / `SwitchStart`) — no new end-of-block op is added. Wire the construct through:

- **lpir**: op definition (`Block`, `ExitBlock`), builder helpers (`push_block`, `end_block`), print/parse, validation, interpreter, const-fold conservative arm. `End` gains a polymorphic arm for `Block` frames (validator / interp).
- **lpvm-native**: region-based lowerer + **new `Region::Block { body, exit_label }` variant**; `ExitBlock` branches to that label; the closing `End` of a `Block` is a no-op marker.
- **lpvm-wasm**: map `Block` to Wasm `block`, the closing `End` to Wasm `end`, and `ExitBlock` to `br <depth>` (depth-to-innermost-block, computed from the existing `wasm_open` / ctrl stack).
- **lpvm-cranelift**: extend control lowering (`emit/control.rs` + `CtrlFrame::Block`) analogous to loop break but without a back-edge.
- **Tests**: minimal cases from roadmap (fall-through, single exit, exit inside `IfStart`, nested blocks).

Validation commands (from roadmap):

- `cargo test -p lpir`
- `cargo test -p lpvm-native`
- `cargo test -p lpvm-wasm`
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server`

## Current state of the codebase

### lpir

- **`lpir_op.rs`**: Control markers include `IfStart` / `Else` / `LoopStart` / `SwitchStart` / … / `End`; jumps include `Break` / `Continue` / `BrIfNot`. No block/exit-block ops yet.
- **`builder.rs`**: `BlockEntry` stack covers `If` / `Else` / `Loop` / `Switch` only; `push_if` / `end_if` patch `IfStart` offsets on close (pattern to mirror for `Block` / `EndBlock`).
- **`validate.rs`**: `StackEntry` tracks structure; `Break` / `Continue` / `BrIfNot` require an innermost `StackEntry::Loop`. `End` pops one stack entry (closes if/else arm, loop, switch arm, etc.). **New**: `StackEntry::Block`, `ExitBlock` finds nearest enclosing block skipping `If` / `Else` / `Loop` / switch arms per roadmap; the existing `End` arm closes a `Block` frame just like it closes any other.
- **`interp.rs`**: `Ctrl` stack has `If` / `Loop` / `SwitchArm`; `Break` pops until `Ctrl::Loop` then jumps to exit index. **New**: `Ctrl::Block { exit }`; `ExitBlock` mirrors `Break` but targets block exit; the existing `End` arm gains a `Some(Ctrl::Block { .. })` case (pop, `pc += 1`).
- **`const_fold.rs`**: Conservative clear list includes control markers; extend for `Block` and `ExitBlock` (no `End`-equivalent op needed).

### lpvm-native

- **`region.rs`**: `Region` is `Linear` | `IfThenElse` | `Loop` | `Seq`. **No** forward-only block yet. Adding a variant will require updating exhaustive consumers:
  - `lower.rs` (build tree + lower jumps)
  - `regalloc/walk.rs`, `regalloc/liveness.rs`
  - `rv32/debug/region.rs` (if present for dumps)
- **`lower.rs`**: `lower_range` already special-cases `IfStart`, `LoopStart`, `Break`, `Continue`, `BrIfNot`, `Else`, `End`. Adds insertion points for `Block` / `ExitBlock`; likely a **`block_stack`** of exit labels (analogous to `LoopFrame`). The closing `End` of a `Block` is consumed by the existing `Else | End => i += 1` arm.

### lpvm-wasm

- **`emit/ops.rs`**: Nested `sink.block` / `Op::End` for `IfStart` and `LoopStart`. Forward `Block` maps cleanly to one extra `block` wrapping the body; `End` (closing a `Block`) emits `sink.end()`; `ExitBlock` → `br` with correct relative depth (innermost forward block, skipping loop/if block nesting per Wasm stack — must match LPIR nesting rules). Tracked via a new `CtrlEntry::Block { outer_open_depth }`.

### lpvm-cranelift

- **`emit/control.rs`** + **`emit/mod.rs`**: Structured lowering with `ctrl_stack`; extend `CtrlFrame` with a `Block { merge_block }` variant; `Block` creates `merge_block`; `ExitBlock` jumps to it; the existing `End` arm closes a `Block` frame (jump-if-unterminated to merge, then `switch_to_block(merge)`).

### Tests / filetests

- Roadmap suggests `.lpir` filetests or `lpir/src/tests/`. **Stage-i** plan already established print/parse/validate/interp patterns under `lpir`. `lps-filetests` is GLSL→IR oriented; raw LPIR filetests may be thin or absent — needs a product decision (see questions).

## Questions (to resolve in chat)

### Q0 — Closer marker: dedicated `EndBlock` vs reuse `End`?

**Context:** Roadmap proposed `EndBlock` as a dedicated closer. Today, `End` already closes `IfStart`, `Else`, `LoopStart`, switch arms, and `SwitchStart` polymorphically — validator pops the stack, interp dispatches on `ctrl.last()`, print emits `}`, parse uses `close_brace_for_text` to dispatch by stack state.

**Resolution:** **Reuse `End`.** Rationale:

- Adding a per-construct closer breaks an established convention used by every other structured op in LPIR.
- The supposed benefit (unambiguous op-stream scan) doesn't exist — disambiguating `End` already requires walking the stack.
- Backends already track structured stacks (`loop_stack`, `ctrl_stack`, `wasm_open`); they don't dispatch on op type for closers.
- Smaller op enum, fewer arms to update across validator/interp/print/parse/lower.

So M2 introduces only **two** new ops: `Block { end_offset }` and `ExitBlock`. Plan/design files updated accordingly.

### Q1 — Primary test surface for M2

**Context:** The roadmap lists four minimal scenarios and allows either dedicated `.lpir` filetests or unit tests in `lpir/src/tests/` if filetests are awkward.

**Suggested answer:** Implement the four scenarios as **`lpir` crate unit tests** (build IR via `FunctionBuilder` or literal `Vec<LpirOp>`, run `validate_function`, `interpret`, and optionally round-trip print/parse). Add **one** backend smoke path (e.g. compile the same module through `lpvm-native` or `lpvm-wasm` in an existing test harness) only where an existing pattern is cheap. Defer **lps-filetests** `.lpir` integration to M4 unless you want parser coverage in filetests now.

*User answer:* **Confirmed.** There are no direct LPIR filetests; **unit tests in `lpir` (and backend tests as needed)** are required to cover this functionality.

### Q2 — `BrIfNot` vs `Block` (validator / semantics)

**Context:** Today `BrIfNot` is validated only inside a `Loop` (same as `Break`). The roadmap does not say whether a forward `Block` should be a valid exit target for `BrIfNot`.

**Suggested answer:** Keep **M2 behavior unchanged**: `BrIfNot` remains **loop-only**. Only `ExitBlock` exits a `Block`. This matches Wasm/Cranelift mental models (conditional exit from a block is usually `br_if`, not the existing `BrIfNot` opcode) and minimizes scope.

*User answer:* **Confirmed — `BrIfNot` stays loop-only.** Conditional exits from a `Block` use `IfStart { cond } / ExitBlock / End`. Rationale:

- M3 inliner doesn't structurally need a "br_if to block" op — `IfStart`/`ExitBlock`/`End` is enough.
- Partially generalizing only `BrIfNot` (while `Break`/`Continue` stay loop-only) leaves an inconsistent jump family. Future work (if perf calls for it) is either a dedicated `ExitBlockIfNot` op or a unified `Br { target } / BrIf { cond, target }` family — not a retrofit of `BrIfNot`.
- Native backend already collapses trivial `IfStart`/`ExitBlock`/`End` to a conditional branch (empty-else fast path in `lower.rs` ~line 740), so the perf delta is small.
- Smaller M2 surface, fewer validator edge cases.

### Q3 — Native `Region` naming and regalloc impact

**Context:** Introducing `Region::Block` (or similar) touches the region tree and regalloc walks.

**Suggested answer:** Add **`Region::Block { body: RegionId, exit_label: LabelId }`** (exact fields TBD in design) and thread it through **`lower.rs`**, **`regalloc/walk.rs`**, **`regalloc/liveness.rs`**, and **`rv32/debug/region.rs`** in the same milestone so the tree stays consistent and tests stay green.

*User answer:* **Confirmed.** Add `Region::Block { body: RegionId, exit_label: LabelId }` modeled on `Region::Loop` but forward-only (no back-edge, no fixpoint in liveness). All five exhaustive-match sites updated in M2:

- `lpvm-native/src/region.rs` — new variant
- `lpvm-native/src/lower.rs` — build the variant + lower `Block`/`EndBlock`/`ExitBlock`, with a `block_stack` of exit labels analogous to `LoopFrame`
- `lpvm-native/src/regalloc/walk.rs` — first-vinst + walk dispatch
- `lpvm-native/src/regalloc/liveness.rs` — forward-only live-in/out (single pass; no fixpoint)
- `lpvm-native/src/rv32/debug/region.rs` — debug dump entry

**Tests are critical** (per user) — every layer (validate, interp, print/parse roundtrip, native lower, native regalloc, wasm emit, cranelift emit) must have at least one focused test for the four roadmap scenarios.

## # Notes

- `end_offset` on `LpirOp::Block` is intentional redundancy (parity with `IfStart`, fast lowering); may be recomputed after transforms later.
- Inliner (M3) will be the main producer; no GLSL surface syntax in M2.
- Reusing `End` instead of a dedicated `EndBlock` op (see Q0) keeps the op enum smaller and aligns with how every other structured op in LPIR is closed.
