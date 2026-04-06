# Milestone V: Matrix invoke and large returns (sret)

## Goal

Matrix-returning functions (`mat3`, `mat4`) can be called from the host JIT test harness and
return correct values. All remaining matrix filetest files pass on `jit.q32`.

## Suggested plan name

`lpir-parity-milestone-v`

## Scope

**In scope:**

- **Host invoke glue** (`lpvm-cranelift/src/invoke.rs`): extend beyond the current 4-word return
  cap. Use Cranelift's `enable_multi_ret_implicit_sret` or equivalent caller-allocated return
  area for functions returning >4 scalar words.
- **Return decode**: flatten sret buffer back into `GlslValue::Mat3` / `Mat4` for filetest
  assertions.
- **Verify mat2** (4 words) still works through the existing path or the new sret path uniformly.
- **ABI validation**: confirm the sret approach works on the host ISA (AArch64 on macOS) and does
  not break `no_std` object emission for RV32 (embedded path doesn't use host invoke, but the
  code must compile).

**Out of scope:**

- LPIR lowering for matrices (already done in earlier milestones / WIP).
- Matrix element stores (Milestone II).
- WASM emit for matrix returns (Milestone VI).
- Matrices as `out` / `inout` function parameters (stretch goal; annotate if not reached).

## Key decisions

- **Uniform sret vs size-dependent path**: Use sret for **all** matrix returns (mat2 included)
  rather than branching on word count. Simpler code, one tested path, negligible performance
  difference in a test harness.
- **Per-platform testing**: The primary dev host is AArch64 macOS. CI also tests x86_64. RV32 is
  object-mode only (no host invoke). Document any platform-specific behavior in `invoke.rs`.

## Deliverables

- Updated `invoke.rs` and `values.rs` in `lpvm-cranelift`.
- Remaining `matrix/mat3/*` and `matrix/mat4/*` files passing on `jit.q32`.
- `builtins/matrix-compmult.glsl`: if `matrixCompMult` is a Naga limitation, annotate
  `@unimplemented`; if an alternative name works, wire it.

## Dependencies

Milestones I–II (relational for matrix `==`/`!=`, matrix element stores for incdec). Matrix LPIR
metadata and lowering from the existing WIP must be committed first.

## Estimated scope

Medium. The invoke logic is concentrated in one file; the main work is understanding Cranelift's
ABI flags and testing on the dev host. ~100-200 lines including tests.
