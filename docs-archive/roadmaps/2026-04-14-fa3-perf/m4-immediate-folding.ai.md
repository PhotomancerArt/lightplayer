# Milestone 4: Constant/Immediate Folding in Lowering

## Goal

Fold single-use constants into immediate operands on binary operations,
matching cranelift's behavior where `iadd v1, v2` with `v1 = iconst 1`
becomes `addi rd, rs, 1`.

## Suggested plan name

`fa3-perf-m4`

## Scope

**In scope**:
- New VInst variants with immediate operands: `AddImm32`, `SubImm32`, and
  potentially others for common RISC-V I-type patterns.
- Emitter support for the new VInst variants (trivial: `addi` encoding
  already exists).
- Folding logic: either at lowering time (check if LPIR operand is a
  single-use `IconstI32` fitting in 12-bit signed immediate) or as a VInst
  peephole pass.
- Regalloc support: the immediate-form VInsts have one fewer use operand,
  reducing register pressure.

**Out of scope**:
- Folding across basic block boundaries.
- Floating-point immediate patterns.
- Changes to the LPIR itself.

## Key decisions

- **Lowering-time vs peephole**: Lowering-time is cleaner (access to LPIR
  use-count information) but couples the lowering to the VInst encoding.
  Peephole is more modular but only catches adjacent `IConst32` + binop.
  Decision deferred to implementation time.

- **Which ops to fold**: Start with `Add32` (most common). `Sub32`, shifts,
  and comparisons are candidates for later.

- **12-bit immediate range**: RISC-V I-type immediates are signed 12-bit
  (-2048 to 2047). Constants outside this range cannot be folded and must
  remain as `IConst32` + register operand.

## Deliverables

- New VInst variants in `vinst.rs`.
- Emitter encoding in `rv32/emit.rs`.
- Folding pass (location TBD).
- Updated filetests showing reduced instruction counts.

## Dependencies

M1-M3 should be completed first so the baseline is stable.

## Estimated scope

Moderate-large. ~100-200 lines across `vinst.rs`, `lower.rs` or `peephole.rs`,
`rv32/emit.rs`, and `fa_alloc/walk.rs` (operand counting).
