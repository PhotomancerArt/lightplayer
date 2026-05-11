# M1.1 — LPIR + Format Prerequisites: Design

## Scope

Pre-work for M2.0 (synthetic `__render_texture`). Two independent tracks
of changes that don't interact within M1.1; they only meet in M2.0.

### Track 1: Narrow memory ops

Add six new LPIR ops for sub-32-bit memory access:

| Op | Width | Direction | Extension |
|----|-------|-----------|-----------|
| `Store8`  | 8-bit  | write | truncate (low 8 bits)  |
| `Store16` | 16-bit | write | truncate (low 16 bits) |
| `Load8U`  | 8-bit  | read  | zero-extend            |
| `Load8S`  | 8-bit  | read  | sign-extend            |
| `Load16U` | 16-bit | read  | zero-extend            |
| `Load16S` | 16-bit | read  | sign-extend            |

Semantics:
- Native-endian (matches existing `Store` / `Load`)
- Stores: `value: VReg` is i32-typed; validator rejects f32
- Loads: result `dst: VReg` is i32-typed; sign-extension determined by op
- Layout: `{ base: VReg, offset: u32, value/dst: VReg }` — same shape as
  existing `Store` / `Load`

Existing 32-bit `Store` and `Load` are unchanged.

### Track 2: Texture formats

Add two new variants to `TextureStorageFormat`:

| Variant | Channels | bpp | `compile_px` return type |
|---------|----------|-----|--------------------------|
| `Rgba16Unorm` (existing) | 4 | 8 | `Vec4`  |
| `Rgb16Unorm` (NEW)       | 3 | 6 | `Vec3`  |
| `R16Unorm` (NEW)         | 1 | 2 | `Float` |

`bytes_per_pixel`, `channel_count`, and `expected_return_type` updated.

## File Structure

```
lp-shader/
├── lpir/src/
│   ├── lpir_op.rs                # UPDATE: 6 new op variants
│   ├── print.rs                  # UPDATE: print arms
│   ├── parse.rs                  # UPDATE: parser arms
│   ├── validate.rs               # UPDATE: vreg-use + dst-type checks
│   ├── interp.rs                 # UPDATE: write/read narrow values
│   ├── lpir_module.rs            # UPDATE: uses_memory recognizes new ops
│   └── tests/all_ops_roundtrip.rs # UPDATE: round-trip + per-backend exec
│
├── lpvm-cranelift/src/emit/
│   └── memory.rs                 # UPDATE: istore8/16, uload8/16, sload8/16
│
├── lpvm-native/src/
│   ├── vinst.rs                  # UPDATE: 6 new VInst variants
│   ├── lower.rs                  # UPDATE: LpirOp → VInst arms (+ unit tests)
│   └── rv32/
│       ├── encode.rs             # UPDATE: encode_sb/sh, encode_lb/lbu/lh/lhu
│       └── emit.rs               # UPDATE: emit arms for new VInsts
│
├── lpvm-wasm/src/emit/
│   └── ops.rs                    # UPDATE: i32.store8/16, i32.load8/16 _u/_s
│
├── lps-shared/src/
│   └── texture_format.rs         # UPDATE: add R16Unorm, Rgb16Unorm
│
└── lp-shader/src/
    ├── engine.rs                 # UPDATE: expected_return_type covers 3 formats
    └── tests.rs                  # UPDATE: validation tests for R16Unorm, Rgb16Unorm
```

## Conceptual Architecture

```
┌────────────────────────────────────────────────────────────────────┐
│  Track 1: Narrow memory ops                                        │
│                                                                    │
│  LpirOp { Store8, Store16, Load8U, Load8S, Load16U, Load16S }      │
│       │                                                            │
│       ├──► lpir: print, parse, validate, interp, uses_memory       │
│       │                                                            │
│       ├──► Cranelift: istore8/16, uload8/16, sload8/16             │
│       │                                                            │
│       ├──► Native RV32: VInst → encode (sb, sh, lb, lbu, lh, lhu)  │
│       │                                                            │
│       └──► WASM: i32.store8/16, i32.load8/16 _u/_s                 │
│                                                                    │
│  Semantics: native-endian, truncate (stores), 0/sign-ext (loads)   │
└────────────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────────────┐
│  Track 2: Texture formats                                          │
│                                                                    │
│  TextureStorageFormat                                              │
│       ├── Rgba16Unorm (existing)                                   │
│       ├── Rgb16Unorm (NEW)                                         │
│       └── R16Unorm (NEW)                                           │
│                │                                                   │
│                └──► compile_px::expected_return_type updated       │
│                     + validation tests                             │
└────────────────────────────────────────────────────────────────────┘
```

The two tracks don't interact in M1.1. M2.0 will combine them when
`__render_texture` synthesizes per-channel `Store16` writes at offsets
derived from the chosen `TextureStorageFormat`.

## Main Components

### `LpirOp` enum (lpir crate)

Six new variants, all sharing the same field layout as existing
`Store` / `Load`:

```rust
Store8  { base: VReg, offset: u32, value: VReg }
Store16 { base: VReg, offset: u32, value: VReg }
Load8U  { dst: VReg, base: VReg, offset: u32 }
Load8S  { dst: VReg, base: VReg, offset: u32 }
Load16U { dst: VReg, base: VReg, offset: u32 }
Load16S { dst: VReg, base: VReg, offset: u32 }
```

`def_vreg`: stores return `None`; loads return `Some(dst)` (grouped with
existing `Load`).

### Backend lowering rules

- **Cranelift** (`lpvm-cranelift/src/emit/memory.rs`):
  store8/16 → `builder.ins().istore8/istore16(...)`;
  load8/16 → `builder.ins().uload8/sload8/uload16/sload16(...)`.
  Reuses `MemFlags::new()` and `operand_as_ptr` like existing `Store`.

- **Native RV32** (`lpvm-native`):
  - New `VInst` variants: `Store8`, `Store16`, `Load8U`, `Load8S`,
    `Load16U`, `Load16S`.
  - New encoders in `rv32/encode.rs`: `encode_sb`, `encode_sh`,
    `encode_lb`, `encode_lbu`, `encode_lh`, `encode_lhu`.
  - New emit arms in `rv32/emit.rs` mirroring `Store32` / `Load32` patterns.
  - Lowering in `lower.rs` mirrors existing `Store` / `Load` arms.

- **WASM** (`lpvm-wasm/src/emit/ops.rs`):
  store8/16 → `i32_store8` / `i32_store16` via `mem_arg0`;
  load8/16 → `i32_load8_u` / `i32_load8_s` / `i32_load16_u` /
  `i32_load16_s`. i32-only (no f32 branch).

### `TextureStorageFormat` (lps-shared)

```rust
pub enum TextureStorageFormat {
    Rgba16Unorm,
    Rgb16Unorm,  // NEW: 3ch, 6 bpp
    R16Unorm,    // NEW: 1ch, 2 bpp
}

impl TextureStorageFormat {
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgba16Unorm => 8,
            Self::Rgb16Unorm => 6,
            Self::R16Unorm => 2,
        }
    }

    pub fn channel_count(self) -> usize {
        match self {
            Self::Rgba16Unorm => 4,
            Self::Rgb16Unorm => 3,
            Self::R16Unorm => 1,
        }
    }
}
```

### `compile_px` validation (lp-shader)

```rust
fn expected_return_type(format: TextureStorageFormat) -> LpsType {
    match format {
        TextureStorageFormat::Rgba16Unorm => LpsType::Vec4,
        TextureStorageFormat::Rgb16Unorm => LpsType::Vec3,
        TextureStorageFormat::R16Unorm => LpsType::Float,
    }
}
```

Validation tests added for each format: accepts matching return type,
rejects mismatched ones.
