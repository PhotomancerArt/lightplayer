# Phase 10: Validate

## Correctness

The FloatStrategy emits identical CLIF instructions to the hardcoded calls
it replaces. The test suite must pass with zero changes.

Primary validation — the GLSL filetests fully cover compiler correctness
and are the fastest way to verify:

```bash
scripts/glsl-filetests.sh
```

Then the compiler unit tests:

```bash
cd lp-shader/lps-compiler && cargo test --features std
```

And the full workspace if needed:

```bash
cargo test --workspace
```

## Compilation check (no_std)

The codegen runs on ESP32 (no_std). Verify it compiles without std:

```bash
cd lp-shader/lps-compiler && cargo check --no-default-features --features core
```

## Grep for remaining hardcoded float ops

After all phases, grep the codegen directory for any remaining direct float
instruction calls that should have been routed through the strategy:

```bash
rg '\.ins\(\)\.(f32const|fadd|fsub|fmul|fdiv|fneg|fabs|fcmp|fmin|fmax|sqrt|floor|ceil|fcvt_from_sint|fcvt_to_sint|fcvt_from_uint|fcvt_to_uint)' \
  lp-shader/lps-compiler/src/frontend/codegen/
```

Any remaining hits should be:

- In test code (acceptable)
- In non-float contexts (false positives — e.g. if there's an integer sqrt,
  though unlikely)
- Missed call sites (fix them)

Also check builtins/helpers.rs — the `get_math_libcall` functions build
signatures with `types::F32`. These are NOT changed in Plan A (they're
Plan C — builtin dispatch). Confirm they're in the expected exclusion list.

## Formatting

```bash
cargo +nightly fmt
```

## Call site count

Verify the total number of changes matches expectations:

| Phase                  | Sites    |
|------------------------|----------|
| 3. Scalar arithmetic   | ~10      |
| 4. Constants           | ~21      |
| 5. Comparisons         | ~16      |
| 6. Math/rounding       | ~35      |
| 7. Composed operations | ~125     |
| 8. Type references     | ~11      |
| 9. Conversions         | ~5       |
| **Total**              | **~223** |

The exact count will vary — the matrix files are hard to count precisely
without doing the work. The important thing is that the grep in the
validation step shows no remaining direct float instruction calls in the
codegen (outside of test code and the numeric.rs FloatStrategy itself).

## What's left after Plan A

The following still use direct float operations and are intentionally
NOT changed:

- `builtins/helpers.rs` — `get_math_libcall` signature building (Plan C)
- `backend/transform/q32/` — the Q32 transform (unchanged, coexists)
- Test code — tests may construct CLIF directly
- `frontend/codegen/numeric.rs` — FloatStrategy delegates to CLIF ops

These will be addressed in subsequent plans (B, C, D).
