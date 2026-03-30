# Milestone I — tests expected to pass (all targets)

This list is the **contract** for stage I: every file below must report **no unexpected failures**
on **`jit.q32`**, **`wasm.q32`**, and **`rv32.q32`** when the milestone is done.

Shared LPIR lowering fixes Cranelift JIT, WASM, and RV32; parity means the **same** `.glsl` files
must pass on all three (unless a line is intentionally `@unsupported`, see below).

## Tier A — fixed corpus (explicit paths)

Run these paths on **each** target (see commands in [`05-phase-cleanup-validation.md`](./05-phase-cleanup-validation.md)):

| # | Path |
|---|------|
| 1 | `builtins/common-isnan.glsl` |
| 2 | `builtins/common-isinf.glsl` |
| 3 | `matrix/mat2/op-equal.glsl` |
| 4 | `matrix/mat2/op-not-equal.glsl` |
| 5 | `matrix/mat3/op-equal.glsl` |
| 6 | `matrix/mat3/op-not-equal.glsl` |
| 7 | `matrix/mat4/op-equal.glsl` |
| 8 | `matrix/mat4/op-not-equal.glsl` |

**Annotations:**

- Remove `@unimplemented(backend=jit)` / per-line markers on `// run:` lines that this milestone
  fixes.
- Remove file-level or per-line `@unimplemented(backend=wasm)` (and `rv32` if present) on these
  files once the WASM / RV32 backends execute the same LPIR correctly — otherwise parity is not met.
- **`@unsupported(float_mode=q32, …)`** is allowed where behavior is **not required** on Q32 by
  design ([`docs/design/q32.md`](../../design/q32.md) §7). Do not use it to hide relational bugs in
  Tier A unless the test truly requires IEEE-only behavior.

## Tier B — bvec / relational triage

Bool vector relational coverage lives mainly under **`filetests/vec/bvec2/`**, **`bvec3/`**,
**`bvec4/`** (not a top-level `bvec/` directory). Some **`uvec*/`** files failed on nested
`all`/`any` (`Expression::Relational`) and belong here too.

During phase 4, **enumerate** every additional file you unmark and fix for relational-only failures.
Append that list to **[`summary.md`](./summary.md)**. Each Tier B file is subject to the **same
three-target bar** as Tier A.

## What “pass” means

- Exit code **0** from `scripts/glsl-filetests.sh --target <t> <paths…>`.
- No unexpected failures: every `// run:` is either green or explicitly `@unsupported` / expected
  failure per annotation rules.

## Full-suite regression

After Tier A (+ Tier B list) pass, run the **full** filetest matrix on all targets (e.g.
`just test-filetests` or three invocations with `--summary`) to catch collateral regressions.
