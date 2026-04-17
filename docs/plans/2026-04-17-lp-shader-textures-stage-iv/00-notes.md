# M1.1 — LPIR + Format Prerequisites: Planning Notes

## Scope of Work

Land the small, self-contained prerequisites for M2.0 (synthetic
`__render_texture`) as a single "grab-bag" plan:

1. **`Store16` LPIR op** — new memory op that stores the low 16 bits of an
   `i32` VReg to `base + offset`. Required to write unorm16 channels to
   texture buffers cleanly.
2. **`R16Unorm` and `Rgb16Unorm` texture formats** — new variants in
   `TextureStorageFormat`. Forces format-parameterized design in M2.0.
3. **`compile_px` validation update** — `expected_return_type` covers all
   three formats with matching render return types.
4. **LPIR programmatic-construction spike** — confirm existing `FunctionBuilder`
   API is sufficient for M2.0's synthetic `__render_texture` synthesis,
   or identify gaps.

## Dependencies

- M1 (pixel shader contract) — done
- `feature/inline` work (stable function IDs) — NOT required for M1.1
  itself (nothing in M1.1 adds new LPIR functions). Required for M2.0.

## Current State of the Codebase

### `Store` op — touch points for parallel `Store16`

Everything that matches `LpirOp::Store` needs a companion `Store16` arm:

**lpir crate (`lp-shader/lpir/src/`):**
- `lpir_op.rs` — enum variant, `def_vreg` impl (groups with `Store`,
  `Memcpy`, etc. — returns `None`)
- `print.rs` — dedicated `Store` arm
- `parse.rs` — text parser (`parse_store`)
- `validate.rs` — vreg-use checks, `mark_op_defs`, opcode-dst-type arm
- `interp.rs` — interpreter writes via `write_u32`
- `lpir_module.rs` — `IrFunction::uses_memory`
- `tests/all_ops_roundtrip.rs` — exercises `Store` in a synthetic function

**Cranelift backend (`lp-shader/lpvm-cranelift/`):**
- `src/emit/memory.rs` — `LpirOp::Store` → `builder.ins().store(MemFlags::new(), val, ptr, offset)`

**Native JIT / RV32 (`lp-shader/lpvm-native/`):**
- `src/lower.rs` — `LpirOp::Store` → `VInst::Store32`
- `src/vinst.rs` — `VInst::Store32` definition (need new `Store16` variant)
- `src/rv32/emit.rs` — `encode_sw` for Store32; need `encode_sh` for Store16
- `src/rv32/encode.rs` — instruction encoders (need RV32 `sh`)
- Regalloc / debug helpers touching `VInst::Store32`

**WASM backend (`lp-shader/lpvm-wasm/`):**
- `src/emit/ops.rs` — `Store` → `i32_store` / `f32_store` via `mem_arg0`;
  need `i32_store16`

**Emulator paths:**
- `lpvm-emu` compiles via `lpvm_cranelift::object_bytes_from_ir` — no
  separate op table, gets Store16 for free from Cranelift
- `rt_emu` runs native RV32 machine code — gets Store16 from
  `lpvm-native` RV32 emission (new `encode_sh` path)

### `TextureStorageFormat` (`lp-shader/lps-shared/src/texture_format.rs`)

Single variant today: `Rgba16Unorm` (4 channels, 8 bpp). Methods to update:
`bytes_per_pixel()`, `channel_count()`. No `Display` / serde impl.
No exhaustive matches elsewhere besides `expected_return_type`.

### `compile_px` validation (`lp-shader/lp-shader/src/engine.rs`)

`validate_render_sig` + `expected_return_type`:

```rust
fn expected_return_type(format: TextureStorageFormat) -> LpsType {
    match format {
        TextureStorageFormat::Rgba16Unorm => LpsType::Vec4,
    }
}
```

Tests in `lp-shader/lp-shader/src/tests.rs` cover existing Rgba16Unorm
validation scenarios (missing render, wrong param count/type, wrong return
type).

### LPIR programmatic construction — existing precedent

`lpir::FunctionBuilder` (`lp-shader/lpir/src/builder.rs`) is already a
well-developed API for building functions programmatically:
- `new(name, return_types)`, `set_entry()`
- `add_param(ty)`, `alloc_vreg(ty)`, `alloc_slot(size)`
- `push(op)` (raw op)
- Structured control flow: `push_if`, `push_else`, `push_end`, loops, switches

`ModuleBuilder` (same file) composes functions into an `LpirModule`:
- `add_import(...)` → `CalleeRef`
- `add_function(ir)` → `CalleeRef`

**Precedent for synthetic functions**: `synthesize_shader_init` in
`lps-frontend/src/lower.rs` (line 236+) builds a synthetic function using
`FunctionBuilder` with global initializers. Pattern is mature.

For M2.0's `__render_texture` synthesis, the existing `FunctionBuilder`
almost certainly has everything needed. The spike is mostly confirmation,
potentially a tiny helper if anything is awkward.

### Filetest infrastructure (`lps-filetests`)

- `DEFAULT_TARGETS` (runs on `cargo test`): `rv32n`, `rv32c`, `wasm`
  (excludes host `jit`)
- `ALL_TARGETS` (CI): `wasm`, `jit`, `rv32c`, `rv32n`

## Questions

### Q1: What are Store16 value semantics for values outside [0, 65535]?

**Context**: `Store16 { base, offset, value: VReg }` stores the low 16 bits
of an `i32` VReg. What happens if the value is out of range?

**Suggested approach**: **Truncate** — store `value as u16` (low 16 bits),
matching WASM `i32.store16` and RV32 `sh` semantics. No clamping, no error.
The caller (M2.0's Q32-to-unorm16 conversion) is responsible for clamping
before the store.

**Answer**: Truncate. Store `value as u16` (low 16 bits). Matches hardware
semantics (WASM `i32.store16`, RV32 `sh`). Zero overhead. Caller clamps.

### Q2: Should Store16 accept `f32`-typed VRegs?

**Context**: Current `Store` accepts both `i32` and `f32` VRegs (WASM
backend branches on type). For Store16, only `i32` makes sense for unorm16
channel writes; f32 → u16 conversion should be explicit (via
`FtoiSatU` + clamp).

**Suggested approach**: **i32 only.** Validator errors on f32 value VReg.
Keeps semantics simple and avoids implicit truncation of float bits.

### Q3: Do we also need Store8?

**Context**: No current consumer for Store8. Future `Rgba8Unorm` etc.
would need it, but nothing in M2.0 or the immediate roadmap does.

**Suggested approach**: **Scope to Store16 only.** Add Store8 when a
concrete consumer arrives.

### Q4: Should M1.1 add a matching `Load16` op?

**Context**: Symmetry with Store. Not needed for M2.0 (texture writes only).
Needed for texture *reads* (M3), which is a separate milestone.

**Suggested approach**: **Store16 only for M1.1.** Add Load16 alongside
texture read support when we get to it.

### Q5: New texture formats — add both R16Unorm and Rgb16Unorm, or just R16Unorm?

**Context**: The design rationale was "force format-parameterized code".
R16Unorm alone forces scalar-vs-vector return unpacking. Rgb16Unorm adds
awkward 3-channel stride (6 bpp).

**Suggested approach**: **Add both.** The cost of adding Rgb16Unorm to
`TextureStorageFormat` is trivial (two enum arms, two match updates,
two tests). Forces M2.0 to handle all stride patterns. Decision already
made in roadmap planning.

### Q6: LPIR builder spike — plan deliverable or skip entirely?

**Context**: The exploration revealed `FunctionBuilder` is mature and
`synthesize_shader_init` is a working precedent. The spike might be a
no-op.

**Suggested approach**: **Lightweight phase**: read `FunctionBuilder` API,
sketch the shape of `__render_texture` synthesis in a short doc (not
compiled code), confirm no gaps. If we find gaps, add helpers. Budget:
half a phase.

Output goes into a `lpir-builder-spike.md` doc in the plan directory.

### Q7: Where do Store16 tests live?

**Context**: `lpir/tests/all_ops_roundtrip.rs` tests round-trip +
backend execution of each op. Cranelift, native JIT, WASM have lowering
unit tests.

**Suggested approach**: 
- Add Store16 to `all_ops_roundtrip.rs` (lpir).
- Unit test in `lpvm-native/src/lower.rs` asserting
  `Store16 → VInst::Store16`.
- Rely on `all_ops_roundtrip.rs` for Cranelift / WASM end-to-end.
- No new filetest — filetests exercise Store16 only once M2.0 lands.

### Q8: Endianness concern for Store16 writes to texture buffers?

**Context**: Host + wasm are little-endian; RV32 targets are LE.
Existing Store (32-bit) already relies on this. Not a new issue.

**Suggested approach**: **No action.** Document that Store16 writes
low 16 bits in native-endian order (LE on all our targets).
