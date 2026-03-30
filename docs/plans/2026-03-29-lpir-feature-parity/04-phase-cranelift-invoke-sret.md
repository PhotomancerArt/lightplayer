# Phase 4: Cranelift host invoke — multi-return and sret

## Scope of phase

Extend **`lpir-cranelift`** host **invoke** (`invoke.rs` and related) so JIT’d functions with
**more than four** 32-bit return words (e.g. `mat3` → 9, `mat4` → 16) match Cranelift’s ABI
(`enable_multi_ret_implicit_sret` or equivalent). This is **Rust caller glue only** — LPIR and
CLIF already express multi-return.

## Code organization reminders

- Isolate ABI-specific code in `invoke.rs` (or a small `invoke_abi.rs` if the file grows).
- Document per-platform behavior (AArch64 vs x86 vs RISC-V host) in module-level comments if
  non-obvious.

## Implementation details

1. **Audit current behavior** — `invoke_i32_args_returns` (or successor) and the 4-word cap;
   identify how `mat2` (4 words) is handled today.

2. **Cranelift flags** — align with the fork’s multi-return / struct-return story; pass a
   caller-allocated buffer when required.

3. **Decode path** — flatten returned scalars into `GlslValue` / filetest expectations for matrix
   types.

4. **Tests**
   - `cargo test -p lpir-cranelift`
   - Filetests: `./scripts/glsl-filetests.sh function/return-matrix.glsl` and representative
     `matrix/mat3/` / `matrix/mat4/` cases.

5. **ESP32 / RV32 object path** — sanity-check that embedded compilation still builds; invoke.rs
   is host-oriented but must not break `no_std` object emission.

## Validate

```bash
cargo test -p lpir-cranelift
cargo test -p lpir-cranelift --features riscv32-emu
./scripts/glsl-filetests.sh matrix/ function/return-matrix.glsl
```

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```
