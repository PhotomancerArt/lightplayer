# Phase 2 — Cranelift backend: narrow memory ops

## Scope

Lower the six new LPIR ops to Cranelift instructions. Single file
update, mirrors the existing `Store` / `Load` lowering one-for-one.

## Code organization reminders

- All new arms live in `lpvm-cranelift/src/emit/memory.rs`.
- Reuse `operand_as_ptr` and `MemFlags::new()` from existing code.
- Keep lowering arms ordered: stores, then loads — same as `lpir_op.rs`.

## Implementation details

### `lpvm-cranelift/src/emit/memory.rs`

Cranelift instruction mapping:

| LPIR op   | Cranelift call                                    |
|-----------|---------------------------------------------------|
| `Store8`  | `builder.ins().istore8(MemFlags::new(), val, ptr, off)`  |
| `Store16` | `builder.ins().istore16(MemFlags::new(), val, ptr, off)` |
| `Load8U`  | `builder.ins().uload8(types::I32, MemFlags::new(), ptr, off)`  |
| `Load8S`  | `builder.ins().sload8(types::I32, MemFlags::new(), ptr, off)`  |
| `Load16U` | `builder.ins().uload16(types::I32, MemFlags::new(), ptr, off)` |
| `Load16S` | `builder.ins().sload16(types::I32, MemFlags::new(), ptr, off)` |

Add arms after the existing `Store` arm and after the existing `Load`
arm in `emit_memory`. Pattern for stores:

```rust
LpirOp::Store8 {
    base,
    offset,
    value,
} => {
    let ptr = operand_as_ptr(builder, vars, ctx, *base);
    let val = use_v(builder, vars, *value);
    builder.ins().istore8(
        MemFlags::new(),
        val,
        ptr,
        i32::try_from(*offset)
            .map_err(|_| CompileError::unsupported("store8 offset does not fit in i32"))?,
    );
}
```

`Store16` is identical but calls `istore16`. Loads always produce
`types::I32` (i32 dst) and select the appropriate `uload8` / `sload8`
/ `uload16` / `sload16`. No need to consult `vreg_types[dst]` — these
ops are i32-only by definition.

Loads do not need `ir_type_for_mode` since the result type is fixed
(`types::I32`). Use `def_v(builder, vars, *dst, val)` to bind it.

### Unit tests

Per-backend coverage lives in phase 5's `all_ops_roundtrip.rs`
extension. No standalone Cranelift unit test is required for this
phase; existing `cargo test -p lpvm-cranelift` must keep passing.

## Validate

```bash
cargo check -p lpvm-cranelift
cargo test  -p lpvm-cranelift
```
