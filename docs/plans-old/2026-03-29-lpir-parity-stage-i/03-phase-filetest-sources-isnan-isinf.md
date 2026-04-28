# Phase 3: `common-isnan.glsl` / `common-isinf.glsl` (Naga-parseable)

## Scope of phase

Edit filetest **sources** so Naga accepts them (no `Float literal is infinite` or similar), while
still exercising `isnan` / `isinf` where possible under Q32 semantics.

## Code organization reminders

- Prefer **constructing** problematic values with expressions Naga accepts (e.g. `0.0/0.0` may
  still error — check Naga rules) or **drop** cases that require IEEE literals.
- Align expectations with [`docs/design/q32.md`](../../design/q32.md): on **q32**, `isnan`/`isinf`
  are **false**; tests should expect `false` / `bvec*` of false, not IEEE truth.

## Implementation details

- Read current `lps-filetests/filetests/builtins/common-isnan.glsl` and `common-isinf.glsl`.
- Replace `1.0/0.0` or infinite literals with:
  - finite values + `isnan`/`isinf` expectations **false**, and/or
  - file-level or per-case `@unsupported(float_mode=q32, …)` **only** where the test truly requires
    IEEE (prefer not for these two files if the milestone goal is “pass on jit.q32”).
- Remove `@unimplemented(backend=jit)` from these files when they pass.

## Validate

Tier A builtins — **all three targets** (Cranelift JIT, WASM, RV32):

```bash
cd lp2025
for t in jit.q32 wasm.q32 rv32.q32; do
  ./scripts/filetests.sh --target "$t" builtins/common-isnan.glsl builtins/common-isinf.glsl
done
```

See [`expected-passing-tests.md`](./expected-passing-tests.md).
