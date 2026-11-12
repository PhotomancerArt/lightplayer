# Phase 3 — Native RV32 backend: narrow memory ops

## Scope

Add VInst variants, encoders, lowering, and emit arms for the six new
LPIR ops in `lpvm-native`. Largest backend phase since it spans the
full lower → vinst → encode → emit pipeline.

## Code organization reminders

- `vinst.rs` keeps a 1:1 mapping between LPIR memory ops and VInsts.
- `rv32/encode.rs` adds RISC-V encoders next to existing `encode_lw`.
- `rv32/emit.rs` adds emit arms next to existing `Load32` / `Store32`.
- `lower.rs` arms grouped with existing `Load` / `Store` arms.

## Implementation details

### `lpvm-native/src/vinst.rs` — six new variants

Add directly after `Store32` (around line 337):

```rust
/// 8-bit store: `[base + offset] = src` (low 8 bits).
Store8 { src: VReg, base: VReg, offset: i32, src_op: u16 },
/// 16-bit store: `[base + offset] = src` (low 16 bits).
Store16 { src: VReg, base: VReg, offset: i32, src_op: u16 },
/// Zero-extending byte load: `dst = u8[base + offset]`.
Load8U  { dst: VReg, base: VReg, offset: i32, src_op: u16 },
/// Sign-extending byte load: `dst = i8[base + offset]`.
Load8S  { dst: VReg, base: VReg, offset: i32, src_op: u16 },
/// Zero-extending halfword load.
Load16U { dst: VReg, base: VReg, offset: i32, src_op: u16 },
/// Sign-extending halfword load.
Load16S { dst: VReg, base: VReg, offset: i32, src_op: u16 },
```

Update every `match` covering `Load32` / `Store32` (current sites at
lines ~377, 399, 402, 447, 448, 490, 491, 553, 556) to include the
six new variants. Patterns mirror the 32-bit forms exactly:

- `src_op` accessor → group with `Store32 { src_op, .. }` /
  `Load32 { src_op, .. }`
- defs: stores → `None`; loads → `Some(*dst)` (group with `Load32`)
- vreg use enumeration → `base` for loads, `(src, base)` for stores
- name strings: `"Store8"`, `"Store16"`, `"Load8U"`, `"Load8S"`,
  `"Load16U"`, `"Load16S"`

### `lpvm-native/src/lower.rs` — six new arms

Add after the existing `Store` arm (around line 322), mirroring it:

```rust
LpirOp::Store8 { base, offset, value } => {
    let off = i32::try_from(*offset).map_err(|_| LowerError::UnsupportedOp {
        description: String::from("Store8: offset does not fit i32"),
    })?;
    Ok(VInst::Store8 {
        src: fa_vreg(*value),
        base: fa_vreg(*base),
        offset: off,
        src_op: po,
    })
}
// ... Store16 ...
LpirOp::Load8U { dst, base, offset } => {
    let off = i32::try_from(*offset).map_err(|_| LowerError::UnsupportedOp {
        description: String::from("Load8U: offset does not fit i32"),
    })?;
    Ok(VInst::Load8U {
        dst: fa_vreg(*dst),
        base: fa_vreg(*base),
        offset: off,
        src_op: po,
    })
}
// ... Load8S, Load16U, Load16S ...
```

Add lower-level unit tests in the existing `lower.rs` test module
(if present) covering one round-trip per op: build a tiny
`IrFunction` with the LPIR op, run `lower`, assert the resulting
`VInst` matches expectations.

### `lpvm-native/src/rv32/encode.rs` — funct3 constants + encoders

Add funct3 constants near existing `F3_LW`:

```rust
const F3_LB:  u32 = 0b000;
const F3_LH:  u32 = 0b001;
const F3_LBU: u32 = 0b100;
const F3_LHU: u32 = 0b101;
const F3_SB:  u32 = 0b000;
const F3_SH:  u32 = 0b001;
```

Add six encoders modeled on `encode_lw` / `encode_sw`:

```rust
pub fn encode_lb (rd: u32, rs1: u32, offset: i32) -> u32 {
    encode_i_type(OP_LOAD,  rd, F3_LB,  rs1, offset)
}
pub fn encode_lbu(rd: u32, rs1: u32, offset: i32) -> u32 {
    encode_i_type(OP_LOAD,  rd, F3_LBU, rs1, offset)
}
pub fn encode_lh (rd: u32, rs1: u32, offset: i32) -> u32 {
    encode_i_type(OP_LOAD,  rd, F3_LH,  rs1, offset)
}
pub fn encode_lhu(rd: u32, rs1: u32, offset: i32) -> u32 {
    encode_i_type(OP_LOAD,  rd, F3_LHU, rs1, offset)
}
pub fn encode_sb (rs2: u32, rs1: u32, offset: i32) -> u32 {
    encode_s_type(OP_STORE, F3_SB, rs1, rs2, offset)
}
pub fn encode_sh (rs2: u32, rs1: u32, offset: i32) -> u32 {
    encode_s_type(OP_STORE, F3_SH, rs1, rs2, offset)
}
```

### Unit tests for encoders

Add to existing `#[cfg(test)] mod tests` (after `encode_add_x1_x2_x3`):
spot-check expected bit patterns. Cross-reference encodings against
the RISC-V ISA manual (Table 24.1).

### `lpvm-native/src/rv32/emit.rs` — six new arms

Add after `VInst::Store32` (around line 624), mirroring its body
exactly. Two-line wrappers around the new encoders. The lower-half
spill / wide-store paths only matter for 32-bit moves; narrow ops
just emit the single sb/sh/lb/lbu/lh/lhu instruction.

For loads, follow `VInst::Load32` (line 612) — guard with
`is_dead_def`, fetch base via `use_vreg`, allocate `def_vreg`, push
the encoded word, then `store_def_vreg`.

For stores, follow `VInst::Store32` (line 621) — fetch `src` and
`base` via `use_vreg`, push the encoded word.

## Validate

```bash
cargo check -p lpvm-native
cargo test  -p lpvm-native --features rt-jit
cargo test  -p lpvm-native --features rt-emu
```

Encoder unit tests verify bit patterns. End-to-end execution coverage
deferred to phase 5.
