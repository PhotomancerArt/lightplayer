# Phase 4 — WASM backend: narrow memory ops

## Scope

Lower the six new LPIR ops to WebAssembly instructions. Single file
update in `lpvm-wasm`, mirroring the existing `Store` / `Load` arms.

## Code organization reminders

- All new arms live in `lpvm-wasm/src/emit/ops.rs`.
- Reuse `memory::mem_arg0` for offset/alignment encoding.
- Use alignment hint `0` for byte ops, `1` for halfword ops (log2 alignment).
- Narrow ops are i32-only — no f32 branch needed.

## Implementation details

### `lpvm-wasm/src/emit/ops.rs`

WASM instruction mapping (via `wasm-encoder`'s `InstructionSink`):

| LPIR op   | WASM op            | mem_arg align hint |
|-----------|--------------------|--------------------|
| `Store8`  | `i32_store8(m)`    | `0`                |
| `Store16` | `i32_store16(m)`   | `1`                |
| `Load8U`  | `i32_load8_u(m)`   | `0`                |
| `Load8S`  | `i32_load8_s(m)`   | `0`                |
| `Load16U` | `i32_load16_u(m)`  | `1`                |
| `Load16S` | `i32_load16_s(m)`  | `1`                |

Add arms after the existing `Store` arm (line ~784). Pattern:

```rust
LpirOp::Store8 { base, offset, value } => {
    let m = memory::mem_arg0(*offset, 0);
    sink.local_get(base.0).local_get(value.0).i32_store8(m);
}
LpirOp::Store16 { base, offset, value } => {
    let m = memory::mem_arg0(*offset, 1);
    sink.local_get(base.0).local_get(value.0).i32_store16(m);
}
```

And after the existing `Load` arm (line ~772):

```rust
LpirOp::Load8U { dst, base, offset } => {
    let m = memory::mem_arg0(*offset, 0);
    sink.local_get(base.0).i32_load8_u(m).local_set(dst.0);
}
LpirOp::Load8S { dst, base, offset } => {
    let m = memory::mem_arg0(*offset, 0);
    sink.local_get(base.0).i32_load8_s(m).local_set(dst.0);
}
LpirOp::Load16U { dst, base, offset } => {
    let m = memory::mem_arg0(*offset, 1);
    sink.local_get(base.0).i32_load16_u(m).local_set(dst.0);
}
LpirOp::Load16S { dst, base, offset } => {
    let m = memory::mem_arg0(*offset, 1);
    sink.local_get(base.0).i32_load16_s(m).local_set(dst.0);
}
```

No `vreg_val_ty` switch is needed — narrow loads are always i32.
Narrow stores accept i32 source by definition; if a caller violates
this invariant, validator (phase 1) catches it before emit.

### Verify wasm-encoder API

Confirm method names against `wasmparser::Operator` / `wasm-encoder`
docs before landing — common alternatives: `i32_store_8`,
`i32_load_8u`, etc. Adjust to whatever the local crate version
exposes (look at how `i32_store` / `i32_load` are spelled in the
existing arms; the new ones follow the same case style).

## Validate

```bash
cargo check -p lpvm-wasm
cargo test  -p lpvm-wasm
```

End-to-end byte-pattern verification (loaded value matches stored
value across each width / signedness) lives in phase 5.
