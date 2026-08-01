---
status: fixed          # in the f32 rig; the Q32 rig is unfixed, see "Remaining"
found: 2026-08-01      # how: writing M7 P5's f32 corpus by copying the rig's shape
fixed: 2026-08-01      # lp-shader/lpvm-native/tests/xt_pipeline_f32.rs, 13 sites
area: lp-shader/lpvm-native (test rigs), lpir::builder
class: test-rig-lies-about-its-subject
related:
  - docs/design/float.md
---
# The Xtensa pipeline rigs pass *parameter* types to `FunctionBuilder::new`, whose argument is *return* types

**Symptom** — none. Every affected test passed, before and after the fix. That
is the whole problem: the rigs compiled modules with a different signature than
the one they described, and nothing could tell.

`FunctionBuilder::new(name, return_types)` takes the function's **return**
types; parameters are added afterwards with `add_param`. Both Xtensa
full-pipeline rigs read the second argument as a parameter list:

```rust
// xt_pipeline_f32.rs, run_float_binop — "f(a: float, b: float) -> float"
let mut fb = FunctionBuilder::new("f", &[IrType::F32, IrType::F32]);
let x = fb.add_param(IrType::F32);
let y = fb.add_param(IrType::F32);
...
fb.push_return(&[out]);            // ONE value returned
```

so the compiled function declares **two** F32 returns and returns one. Two
sites were additionally wrong about the *type*: `itof_signed_and_unsigned_…`
and `both_register_pools_saturated_simultaneously` return a float and declared
`[I32]`.

**Why it stayed invisible** — the rig's own harness reads one word. `run_f32`
returns `RunOutcome::Ok(lo)`, the low result register, and discards `hi`. A
declared-but-unwritten second return lands in a register nobody looks at.

**How it surfaced** — M7 P5's `xt_corpus::F32_CASES` was written by copying the
rig's module shapes, and the corpus *does* compare the full return vector,
through `call_f32_words`. The first run reported

```
expected [3F800000]
got      [3F800000, 3F800000]
```

on rv32, which is a two-word return of a one-word value: `1.0f32` in `a0`, and
`a1` holding a copy rather than anything meaningful.

## Why it matters beyond tidiness

`a_float_function_has_the_same_frame_shape_as_before` is an **M7 D7 gate**: it
asserts a float-using function still has exactly one `entry`, one `retw`, and a
32-byte window-overflow reservation, which is the argument that makes the
depth-100 recursion test safe. It was pinning the frame of a function with the
wrong return arity — a shape the compiler will never be asked to emit. The
assertion held either way, but it was not evidence about the real shape until
this was fixed.

## The fix

Each rig site now declares what its function actually returns, and the sigs it
builds already agreed with that (the `LpsModuleSig` half was correct
throughout — only the LPIR half was wrong, which is why the two never
cross-checked). All 16 `xt_pipeline_f32.rs` tests pass unchanged afterwards, so
this corrected the description without moving any behaviour.

## Remaining

`lp-shader/lpvm-native/tests/xt_pipeline.rs` — the older Q32 rig — has the same
error at `binary_module` (`&[IrType::I32, IrType::I32]` for a one-int return).
It is where the f32 rig inherited the pattern. Left unfixed here to keep M7
P5's diff to its own subject; it is a mechanical change with the same expected
outcome (no behaviour moves).

## The generalisable bit

`FunctionBuilder` has no assertion that `push_return`'s arity matches
`return_types`, and `LpsModuleSig` — which *did* describe these functions
correctly — is never cross-checked against the LPIR it accompanies. Two
descriptions of one signature, disagreeing silently, in the crate whose job is
to get signatures right. A debug assertion in `FunctionBuilder::finish` would
have caught every instance of this at the first test run.
