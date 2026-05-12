# Phase 1 — LPIR core: narrow memory ops

## Scope

Add six new LPIR ops in the `lpir` crate. No backend wiring yet — those
land in phases 2-4. This phase makes the IR aware of the ops and gets
the in-process interpreter executing them.

## Code organization reminders

- Group new variants next to existing `Store` / `Load` for locality.
- Match arms for new ops should mirror the existing 32-bit forms.
- Keep `validate.rs` and `lpir_module.rs` `uses_memory` consistent.

## Implementation details

### `lpir/src/lpir_op.rs`

Add six variants directly after `Load` / `Store` in the `LpirOp` enum:

```rust
/// 8-bit store: writes the low 8 bits of `value` to `[base + offset]`.
Store8 {
    base: VReg,
    offset: u32,
    value: VReg,
},
/// 16-bit store: writes the low 16 bits of `value` to `[base + offset]`.
Store16 {
    base: VReg,
    offset: u32,
    value: VReg,
},
/// 8-bit zero-extending load: `dst = u8[base + offset]`.
Load8U {
    dst: VReg,
    base: VReg,
    offset: u32,
},
/// 8-bit sign-extending load: `dst = i8[base + offset]` (sign-extended to i32).
Load8S {
    dst: VReg,
    base: VReg,
    offset: u32,
},
/// 16-bit zero-extending load.
Load16U {
    dst: VReg,
    base: VReg,
    offset: u32,
},
/// 16-bit sign-extending load.
Load16S {
    dst: VReg,
    base: VReg,
    offset: u32,
},
```

Update `def_vreg`:

- Stores → `None` (group with existing `Store`)
- Loads  → `Some(*dst)` (group with existing `Load`)

### `lpir/src/print.rs`

Mnemonics:

| Op       | Text form                              |
|----------|----------------------------------------|
| `Store8`  | `store8 vBASE, OFFSET, vVAL`           |
| `Store16` | `store16 vBASE, OFFSET, vVAL`          |
| `Load8U`  | `vDST = load8u vBASE, OFFSET`          |
| `Load8S`  | `vDST = load8s vBASE, OFFSET`          |
| `Load16U` | `vDST = load16u vBASE, OFFSET`         |
| `Load16S` | `vDST = load16s vBASE, OFFSET`         |

Add stores to the explicit `match` block alongside `LpirOp::Store`
(around line 303). Add loads to `print_simple_op` alongside
`LpirOp::Load` (around line 542).

### `lpir/src/parse.rs`

In `parse_statement` (around line 540), recognize new prefixes before
falling into the assign-RHS path:

```rust
if line.starts_with("store8 ") {
    return parse_store_n(fb, line, "store8 ", StoreWidth::U8);
}
if line.starts_with("store16 ") {
    return parse_store_n(fb, line, "store16 ", StoreWidth::U16);
}
```

Refactor `parse_store` into `parse_store_n` parameterized by width.
Same pattern for the 32-bit `Store` (now `StoreWidth::U32`).

In `parse_assign_rhs` op-name match (around line 1014), add arms for
`load8u`, `load8s`, `load16u`, `load16s` constructing the matching
variants.

### `lpir/src/validate.rs`

Three sites to update — group new variants with existing Store / Load:

1. Use-checking arm (around line 549):

```rust
LpirOp::Store { base, value, .. }
| LpirOp::Store8  { base, value, .. }
| LpirOp::Store16 { base, value, .. } => {
    check(*base, "store base");
    check(*value, "store value");
}
LpirOp::Load    { base, .. }
| LpirOp::Load8U { base, .. }
| LpirOp::Load8S { base, .. }
| LpirOp::Load16U { base, .. }
| LpirOp::Load16S { base, .. } => check(*base, "load base"),
```

2. Dst-type expectations (around line 729): all six new ops grouped
   into the existing `Load { .. } | Store { .. } | ...` arm. Loads
   produce `IrType::I32`; stores have no dst.

3. Defined-set marking (around line 814-821): stores into the
   no-dst arm, loads into the `mark(*dst)` arm.

### `lpir/src/interp.rs`

Mirror the existing `Load` / `Store` arms (around lines 694, 709).
Use `read_u8`, `read_u16`, `write_u8`, `write_u16` helpers — add them
to the helpers section of `interp.rs` if absent. Loads produce
`Value::I32`; sign-extension uses `i8 as i32` / `i16 as i32`,
zero-extension uses `u8 as i32` / `u16 as i32`.

### `lpir/src/lpir_module.rs`

Extend `uses_memory` (around line 76) to recognize the six new ops:

```rust
matches!(
    op,
    LpirOp::Load { .. }
        | LpirOp::Load8U { .. } | LpirOp::Load8S { .. }
        | LpirOp::Load16U { .. } | LpirOp::Load16S { .. }
        | LpirOp::Store { .. }
        | LpirOp::Store8 { .. } | LpirOp::Store16 { .. }
        | LpirOp::SlotAddr { .. }
        | LpirOp::Memcpy { .. }
)
```

### Unit tests (in `interp.rs` / `tests.rs` per existing convention)

Six small tests, one per op, all using a single 16-byte slot:

- `store8` then `load8u` → check truncation + zero-extension
- `store8` then `load8s` → write `0x80`, expect `-128i32`
- `store16` then `load16u` → check truncation + zero-extension
- `store16` then `load16s` → write `0x8000`, expect `-32768i32`
- offset variants (e.g. write at offset 1, read at offset 1)
- print-and-reparse round-trip for each new op (in `tests.rs`)

## Validate

```bash
cargo check -p lpir
cargo test  -p lpir
```

All existing tests continue to pass; new tests cover narrow ops in
isolation.
