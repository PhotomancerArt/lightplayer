# M2 — Block / EndBlock / ExitBlock LPIR Ops

Add a structured forward-jump construct to LPIR for handling multi-return
inlined functions. Unlike `LoopStart`, there is no back-edge — `ExitBlock`
always jumps forward to `EndBlock`.

## Motivation

When inlining a callee with multiple `Return` statements, each return must
jump to a single merge point (the end of the inlined body). Without a
dedicated construct, the only option is to wrap the body in a `LoopStart` +
`Break` to simulate forward jumps. That has two problems:

1. It misleads the native lowerer into allocating loop regions with
   back-edge tracking, costing extra instructions.
2. It's a semantic lie — there is no loop, just a multi-exit region.

`Block`/`EndBlock`/`ExitBlock` is the clean solution. It maps directly to
WebAssembly's `block`/`end`/`br` construct.

## New ops

```rust
/// Forward-only block: ExitBlock jumps to the matching EndBlock.
/// end_offset is the body index of the matching EndBlock.
Block {
    end_offset: u32,
},

/// End marker for a Block (like End for IfStart/LoopStart).
EndBlock,

/// Forward jump to the nearest enclosing Block's EndBlock.
/// Analogous to Break for LoopStart, but no back-edge exists.
ExitBlock,
```

### Structural rules

- `Block` / `EndBlock` are paired (like `IfStart` / `End`).
- `ExitBlock` always exits the nearest enclosing `Block`.
- Nesting with other control flow is allowed: `Block` inside `IfStart`,
  `IfStart` inside `Block`, etc.
- `ExitBlock` skips over intermediate `IfStart`/`LoopStart` when searching
  for its enclosing `Block` (same as `Break` skips `IfStart` to find its
  `LoopStart`).

### Example: inlined two-return function

Before inlining:
```
fn helper(x: i32) -> i32 {
    if x > 0 { return x; }
    return -x;
}
```

After inlining into caller (with remapped vregs):
```
Block { end_offset: 7 }
  IfStart { cond: v5, else_offset: 4, end_offset: 5 }
    Mov { dst: v_result, src: v_x }
    ExitBlock
  Else
  End
  Mov { dst: v_result, src: v_neg_x }
EndBlock
// ... caller continues using v_result ...
```

## Changes by file

### `lpir` crate

| File | Change |
|------|--------|
| `lpir_op.rs` | Add `Block`, `EndBlock`, `ExitBlock` variants. Update `def_vreg()` to return `None` for all three. |
| `builder.rs` | Add `push_block()`, `push_end_block()`, `push_exit_block()` on `FunctionBuilder`. Block offset patched on `push_end_block` (same pattern as `push_end` for `IfStart`). |
| `print.rs` | Print `block {`, `end_block`, `exit_block`. Follow existing indentation style. |
| `parse.rs` | Parse `block {`, `end_block`, `exit_block`. |
| `validate.rs` | Validate pairing (Block must have matching EndBlock, ExitBlock must be inside a Block). Add to control-flow stack tracking. |
| `interp.rs` | `Block`: push `Ctrl::Block { exit }`. `EndBlock`: pop. `ExitBlock`: pop until `Ctrl::Block`, jump to exit. |
| `const_fold.rs` | Add `Block`/`EndBlock`/`ExitBlock` to the conservative-clear arm (same as `IfStart`/`End`/etc.). |

### `lpvm-native` crate

| File | Change |
|------|--------|
| `lower.rs` | Handle `Block`/`EndBlock`/`ExitBlock` in the region-based lowerer. `Block` creates a `Region::Block` with a single exit label. `ExitBlock` emits a branch to the exit label. `EndBlock` is a no-op marker (like `Else`/`End`). |

### `lpvm-wasm` crate

| File | Change |
|------|--------|
| `emit/ops.rs` | Map `Block` → wasm `block`, `EndBlock` → wasm `end`, `ExitBlock` → wasm `br 0` (or appropriate depth). Trivial — this is exactly how wasm blocks work. |

### `lpvm-cranelift` crate

| File | Change |
|------|--------|
| Lowerer | Map to Cranelift `Block` + `jump`. Straightforward since Cranelift has native block support. |

## Filetests

Add a few dedicated filetests for the new construct, written as `.lpir`
text (if supported) or as GLSL that the inliner will transform (in M4).
If the filetest format doesn't support raw LPIR, test via unit tests in
`lpir/src/tests/`.

Minimal test cases:
1. Single-entry block with no ExitBlock (fall-through).
2. Block with one ExitBlock (skip tail).
3. Block with ExitBlock inside an IfStart (conditional skip).
4. Nested Blocks with ExitBlock targeting the inner block.

## Validation

```bash
cargo test -p lpir
cargo test -p lpvm-native
cargo test -p lpvm-wasm
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

## Notes

- `end_offset` on `Block` is redundant (computable by walking forward to
  the matching `EndBlock`). Include it for consistency with `IfStart` and
  to avoid O(n) scans during lowering. Can always be recomputed after
  transformations.
- The inliner (M3) will be the primary producer of `Block` ops. No GLSL
  construct maps to it directly — it's a compiler-internal construct.
