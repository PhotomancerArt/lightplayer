# Phase 3: F32 mode error handling

## Scope of phase

Ensure every float op that has Q32 lowering also fails in non-Q32 (`FloatMode::F32`) mode with a single, clear error message. Remove stale milestone references (e.g. "M1") from the message.

## Code Organization Reminders

- Prefer one catch-all `match` arm listing all unsupported-in-F32 ops over duplicating the same `Err` in many arms.
- Keep the error text stable enough that tests can assert on it if you add a test.

## Implementation Details

In `lower_op`, after all Q32-specific float arms:

1. Extend the existing catch-all that currently matches `Fadd | Fsub | Fmul | FconstF32` to also include:
   - `Fdiv`
   - `Feq | Fne | Flt | Fle | Fgt | Fge`

2. Set `LowerError::UnsupportedOp` description to something like:

   `float op requires Q32 mode (F32 not supported on rv32)`

(Exact wording per plan decision in `00-notes.md`.)

3. If any float op is missing from either the Q32 branch or this catch-all, `lower_op` may fall through to the final `other =>` arm; grep `Op::F` in `lpir/src/op.rs` and ensure coverage.

### Tests

- Extend or add a test that `lower_op` with `FloatMode::F32` returns `Err` for at least one float op (e.g. `Fadd` or `Fdiv`) and that the message contains `Q32` (or matches the chosen string).

## Validate

```bash
cargo test -p lpvm-native --lib
cargo +nightly fmt -p lpvm-native
```
