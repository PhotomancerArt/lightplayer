---
status: fixed          # both rigs, plus the assertion that makes it non-recurring
found: 2026-08-01      # how: writing M7 P5's f32 corpus by copying the rig's shape
fixed: 2026-08-01      # xt_pipeline_f32.rs (13 sites), xt_pipeline.rs (1 site),
                       # lpir::builder::FunctionBuilder::push_return (the guard)
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

None. `lp-shader/lpvm-native/tests/xt_pipeline.rs` — the older Q32 rig, and
where the f32 rig inherited the pattern — had the same error at the single site
`binary_module` (`&[IrType::I32, IrType::I32]` for a one-int return); it now
declares `&[IrType::I32]`. All 41 of that rig's tests pass unchanged, matching
the f32 rig's 16: the correction moved no behaviour, only the description.
`unary_module` and the four call/recursion helpers in the same file were
already right, coincidentally — one param, one return, same type.

## The generalisable bit

`FunctionBuilder` had no assertion that `push_return`'s arity matches
`return_types`, and `LpsModuleSig` — which *did* describe these functions
correctly — is never cross-checked against the LPIR it accompanies. Two
descriptions of one signature, disagreeing silently, in the crate whose job is
to get signatures right.

That gap is now closed by a `debug_assert_eq!` in
`FunctionBuilder::push_return` (`lp-shader/lpir/src/builder.rs`), which fires
the moment a function returns a different number of values than it declared.
It lands in `push_return` rather than the `finish` this note originally
proposed: both catch every instance, but `push_return` names the offending call
site instead of the function that contains it, and costs one comparison rather
than a walk of the body. sret functions satisfy it as they stand — empty
`return_types`, `push_return(&[])`.

It is not merely a guard against a repeat: it is *retroactive* evidence. Every
production caller passes with it enabled — the naga and lps-glsl lowerers, the
LPIR parser, the synth builders, the rt_emu and wasmtime shims — across the
full 850-file / 31,572-assertion default filetest corpus. That is the
cross-check that was missing, and it says nothing else in the shipping tree was
describing a signature it did not build.

It did fire on two callers, both in `lpvm`'s
`validate_render_texture_tests::make_ir_fn_with_param_types`, which built a
function declaring `[I32]` and returning nothing. That one is not a mistake:
those tests feed deliberately-malformed IR to `validate_render_texture_sig_ir`
/ `validate_render_samples_sig_ir`, whose whole job is to reject a render entry
point that declares a return type. The helper now stamps `return_types` onto
the finished `IrFunction` instead of routing the mismatch through the builder —
the subject there is the validator, not the builder. Worth noting as the shape
of the exception: an assertion like this one is a claim about *well-formed*
construction, so the legitimate way past it is to construct the bad IR
directly, never to weaken the assertion.
