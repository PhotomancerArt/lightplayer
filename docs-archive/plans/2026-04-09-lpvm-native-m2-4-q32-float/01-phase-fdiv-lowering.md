# Phase 1: Fdiv lowering

## Scope of phase

Add Q32 lowering for `Op::Fdiv` to `VInst::Call` targeting `__lp_lpir_fdiv_q32`, mirroring existing `Fadd` / `Fsub` / `Fmul` arms in `lower_op`.

## Code Organization Reminders

- Keep float Q32 arms grouped together after `Fmul` and before `FconstF32`.
- Place new tests at the bottom of the `#[cfg(test)] mod tests` block in `lower.rs`.
- No temporary code; no debug prints.

## Match semantics (F32 / non-Q32 mode)

The arm below uses a **guard**: `if float_mode == FloatMode::Q32`. When that guard is **false** (e.g. `FloatMode::F32`), this arm does **not** match; Rust continues matching later arms.

If `Op::Fdiv` is not also listed in the explicit “float requires Q32” catch-all (see Phase 3), `Fdiv` in F32 mode falls through all the way to the final `other => Err(...)` arm, which uses `format!("{other:?}")` — a vague error.

**Requirement:** Ship Phase 3 in the **same PR / change-set** as Phases 1–2, or add `Op::Fdiv` to the F32 catch-all in the **same edit** as the new Q32 `Fdiv` arm.

## Implementation Details

In `lp-shader/lpvm-native/src/lower.rs`, inside `lower_op`, after the `Op::Fmul` Q32 arm:

```rust
Op::Fdiv { dst, lhs, rhs } if float_mode == FloatMode::Q32 => Ok(VInst::Call {
    target: SymbolRef {
        name: String::from("__lp_lpir_fdiv_q32"),
    },
    args: alloc::vec![*lhs, *rhs],
    rets: alloc::vec![*dst],
    callee_uses_sret: false,
    src_op,
}),
```

Do not add special division-by-zero logic; `__lp_lpir_fdiv_q32` in `lps-builtins` already defines behavior.

### Tests

- Add `lower_q32_fdiv_to_call`: assert `Op::Fdiv` in Q32 mode becomes `VInst::Call` with `target.name == "__lp_lpir_fdiv_q32"`, args `[lhs, rhs]`, rets `[dst]`.

## Validate

```bash
cargo test -p lpvm-native --lib lower_q32_fdiv
cargo +nightly fmt -p lpvm-native
```

Optional smoke: `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server` (touches shader path).
