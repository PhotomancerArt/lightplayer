# Phase 5: Final Consolidations

## Goal

Two small dead-code/duplication cleanups that don't fit neatly into the
earlier phases:

1. **Dedupe `EmittedCode`** — there are currently two structs by that
   name. Keep one.
2. **Delete `emit::emit_vinsts`** — marked DEPRECATED in a doc comment;
   verified to have no callers anywhere in the workspace.

(Q7-c, the SRET threshold replacement, was folded into Phases 2 and 3.)

## Steps

### 5.1 Dedupe `EmittedCode`

Today:

- `crate::emit::EmittedCode` (the public, canonical type) — has `code`,
  `relocs`, `debug_lines`, **and** `alloc_output: AllocOutput`.
- `crate::isa::rv32::emit::EmittedCode` — raw output of `emit_function`,
  has `code`, `relocs`, `debug_lines` (no `alloc_output`).

The orchestrator in `emit.rs::emit_lowered` calls
`crate::isa::rv32::emit::emit_function`, gets the inner `EmittedCode`,
then wraps it into the outer `EmittedCode` to attach `alloc_output`.

Refactor:

1. Rename `crate::isa::rv32::emit::EmittedCode` → `Rv32EmitOutput`. Make
   it `pub(crate)` (or `pub(super)`). It's an internal hand-off, not a
   public type.
2. `emit_function` returns `Rv32EmitOutput`.
3. `emit_lowered` builds the canonical `crate::emit::EmittedCode` directly
   from the `Rv32EmitOutput` plus the `AllocOutput`. Same fields, just
   one wrap step instead of two-types-and-a-conversion.

After the refactor, the only public `EmittedCode` is `crate::emit::EmittedCode`.

```
rg 'struct EmittedCode' lp-shader/lpvm-native/src
# Should produce exactly ONE match (in src/emit.rs).
```

### 5.2 Delete `emit::emit_vinsts`

Verified pre-plan: zero callers outside its own definition and a
re-export in `lib.rs`.

1. Delete the function `emit::emit_vinsts` (currently at
   `lp-shader/lpvm-native/src/emit.rs:117-149`).
2. Delete the re-export from `lp-shader/lpvm-native/src/lib.rs:45`:
   ```rust
   pub use emit::{emit_lowered_with_alloc, emit_vinsts, EmittedCode};
   //                                       ^^^^^^^^^^^ remove this
   ```
3. If there's a unit test exercising `emit_vinsts` only (an internal
   self-test, no other purpose), delete it too. If a test happens to
   exercise both `emit_vinsts` and `emit_lowered`, port it to use only
   `emit_lowered`.

### 5.3 Final cross-check

```
rg 'emit_vinsts' lp-shader/lpvm-native
# Should produce ZERO matches.

rg 'crate::isa::rv32' lp-shader/lpvm-native/src --type rust \
    | rg -v 'src/isa/rv32/' \
    | rg -v 'src/.*\btests?\b.*' \
    | rg -v '#\['
# Should produce zero (or only a small number of expected, justifiable)
# matches: tests are allowed to construct ABIs via func_abi_rv32; the
# rv32 leaf modules naturally reference themselves.

cargo check -p lpvm-native
cargo test -p lpvm-native --all-features
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo build -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

### 5.4 Smoke test on-device (recommended, not required)

If hardware is convenient, flash and run the rainbow shader. The
expected outcome is unchanged from
`docs/design/native/perf-report/perf-notes.md` (2026-04-14 entry):

- Compile time: ~565 ms for `rainbow.shader`
- Runtime: ~29-30 FPS
- Peak heap usage: ~136 KB

Any regression is a bug, not a refactor side-effect — this plan made no
behavior changes. Investigate before considering Phase 5 complete.

```bash
espflash flash --chip esp32c6 -T lp-fw/fw-esp32/partitions.csv \
    target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32
espflash monitor --chip esp32c6
```

## Validation

- Exactly one `struct EmittedCode` in the entire crate (in `src/emit.rs`)
- `emit_vinsts` is gone; no references anywhere
- `cargo check -p lpvm-native` clean
- `cargo test -p lpvm-native` all green
- ESP32 firmware compiles and (if tested) runs without regression
- `rg 'crate::isa::rv32' lp-shader/lpvm-native/src` outside of
  `src/isa/rv32/` and tests should now be empty (or very nearly so —
  `link.rs` may legitimately reference `crate::isa::rv32::link` for
  dispatch, which is fine)

## Wrap-up

When this phase lands, `lpvm-native` has the property that:

- `IsaTarget` is the single source of truth for "which ISA are we
  targeting?"
- All ABI-shape information is reachable through `FuncAbi`.
- All target invariants (pool order, register names, alignment, ELF
  metadata, SRET threshold) are reachable through `IsaTarget`.
- `regalloc/`, `lower.rs`, `compile.rs`, `emit.rs`, `crate::abi::frame`,
  `link.rs`, and `rt_jit/` import nothing from `crate::isa::rv32::*`.
- The `crate::isa::rv32::*` leaf is internal — nobody outside it knows
  RV32 details.
- One `EmittedCode` type. No deprecated dead code.

A future ARM port becomes "add `IsaTarget::Thumbv8mMain`, add
`crate::isa::arm/`, fill in the eight `IsaTarget` methods, write the
emitter and `crate::isa::arm::link::patch_call_*`." No structural
refactor needed at that point — exactly the point of this cleanup.
