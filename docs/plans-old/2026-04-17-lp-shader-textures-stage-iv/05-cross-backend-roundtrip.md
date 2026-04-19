# Phase 5 — Cross-backend round-trip tests

## Scope

Extend `lpir/src/tests/all_ops_roundtrip.rs` to cover the six new
narrow memory ops on every backend. Validates that lower → emit →
execute matches the LPIR interpreter's reference behavior end-to-end.

## Code organization reminders

- Single test file, one helper function builds the LPIR.
- Existing helper structure (alloc slot, store + load) is the model.
- Group new ops with the existing `Store` / `Load` block (around line 200).

## Implementation details

### `lpir/src/tests/all_ops_roundtrip.rs`

Extend the body builder so each backend exercises the six new ops in
addition to existing coverage. Two patterns to add:

**Pattern A — narrow store + narrow zero-ext load round-trip:**

For each width (8, 16):

```rust
let v_8u = b.alloc_vreg(IrType::I32);
b.push(LpirOp::IconstI32 { dst: v_8u, value: 0xAB });
b.push(LpirOp::Store8 { base, offset: 4, value: v_8u });
let r_8u = b.alloc_vreg(IrType::I32);
b.push(LpirOp::Load8U { dst: r_8u, base, offset: 4 });
// later: assert r_8u == 0xAB
```

**Pattern B — sign-extension correctness:**

```rust
let v_neg = b.alloc_vreg(IrType::I32);
b.push(LpirOp::IconstI32 { dst: v_neg, value: 0x80 });
b.push(LpirOp::Store8 { base, offset: 5, value: v_neg });
let r_8s = b.alloc_vreg(IrType::I32);
b.push(LpirOp::Load8S { dst: r_8s, base, offset: 5 });
// later: assert r_8s == -128
```

Repeat Patterns A & B for `Store16` / `Load16U` / `Load16S` with
values `0xABCD` (zero-ext) and `0x8000` (sign-ext expected `-32768`).

Returned values: extend the function's return list (or store results
into known slot offsets and have the test framework read them back).
Use whichever pattern existing tests already use to surface results.

### Existing slot reuse

The fixture already allocates a 16-byte slot for `Store32` /
`Load32` / `Memcpy`. Add `+8` byte slot or extend to 32 bytes to fit
the new offsets cleanly. Layout:

| Offset | Use                      |
|--------|--------------------------|
| 0      | existing Store/Load f32  |
| 4      | Store8 / Load8U          |
| 5      | Store8 / Load8S          |
| 8      | Store16 / Load16U        |
| 10     | Store16 / Load16S        |

### Per-backend test functions

The file already runs each backend (interpreter, Cranelift, Native,
WASM) against the assembled module. The new ops automatically flow
through each backend's lowering — no per-backend test plumbing change
required.

If any backend test gates on feature flags (e.g. `#[cfg(feature = "rt-jit")]`),
preserve those gates verbatim.

### Print/parse round-trip coverage

The fixture also asserts print → parse → print stability. Adding the
new ops to the fixture body automatically exercises that path too.

## Validate

```bash
cargo test -p lpir
cargo test -p lpir --features cranelift   # if gated
cargo test -p lpvm-cranelift
cargo test -p lpvm-native --features rt-jit
cargo test -p lpvm-native --features rt-emu
cargo test -p lpvm-wasm
```

All round-trip values match across backends; sign-extension matches
LPIR interpreter reference.
