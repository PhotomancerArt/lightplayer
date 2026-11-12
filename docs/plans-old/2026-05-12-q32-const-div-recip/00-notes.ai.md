# Q32 Const-Div Fast Path Plan Notes

## Scope

Plan the next focused math-perf pass for Q32 division in the JIT hot path.

Primary targets:

- Add an LPIR representation for float division by a compile-time constant.
- Have the frontend emit that representation where constants are easy to see.
- Have the native backend lower Q32 constant division to the fastest pragmatic runtime path.
- Continue collapsing normal rendering toward a single fast-math product path.

Secondary targets:

- Keep filetests cycle counts tight enough to compare const-div and dynamic-div behavior.
- Leave clear follow-up hooks for general dynamic inline div and debug math probes.

Explicitly out of scope:

- Full GLSL constant propagation beyond what the frontend can cheaply see.
- CSE/LICM for repeated dynamic divisors.
- Debug probe implementation.
- Preserving exact/reference math in normal render mode.

## User Context

- The previous perf pass made Q32 fast modes the default and replaced the old Taylor sine helper.
- Hardware profile after that pass showed division is now the highest remaining math helper in both `examples/basic` and Rocaille.
- The desired product direction is fast-only normal rendering. Reference/correctness behavior should later move to debug probes, not ordinary compiler modes.
- User is comfortable leaning hard toward performance for shader code, even when fixed-point edge semantics get less exact.
- User prefers a pragmatic compiler representation if it avoids hidden backend pattern detection complexity.
- Filetests are useful here because they provide quick per-test cycle counts.
- Hardware is available at `/dev/cu.usbmodem1101` for ESP32-C6 measurements when implementation time comes.

## Current Codebase State

- `lpvm-native` compiles each function through:
  - clone LPIR function
  - `lpir::const_fold::fold_constants(&mut func_opt)`
  - `lpvm-native::lower::lower_ops(...)`
  - immediate folding
  - register allocation/emission
- `lpir::const_fold` currently tracks `IconstI32` and folds integer operations. It does not track `FconstF32` and does not rewrite Q32 float operations.
- `lps-frontend::lower_binary_vec` receives Naga expression handles for both sides of a binary op before lowering operands. This is a good pragmatic place to detect RHS literals/module constants and emit a semantic const-div LPIR op.
- In Q32 mode, `lpvm-native::lower` turns `FconstF32` into an `IConst32` VInst by `((*value as f64) * 65536.0) as i32`.
- In Q32 mode, `lpvm-native::lower` currently lowers all `Fdiv` as a helper call:
  - `DivMode::Saturating` -> `BuiltinId::LpLpirFdivQ32`
  - `DivMode::Reciprocal` -> `BuiltinId::LpLpirFdivRecipQ32`
- Q32 wrapping `Fmul` is already inline in native lowering as a 5-VInst `mul`/`mulh`/`srli`/`slli`/`or` sequence.
- `lpvm-wasm` already emits reciprocal divide inline for Q32 reciprocal mode, but it does not special-case constant divisors.
- `CompilerConfig` and model `GlslOpts` still carry Q32 math modes. They are now transitional compatibility surfaces rather than the intended long-term product model.

## Resolved Design Questions

### Q1: Where should const-div be represented?

Decision:

- Add a semantic LPIR operation such as `FdivConstF32 { dst, lhs, rhs }`.

Why:

- Backend-only pattern detection is internally clean but hidden and awkward around lowering state.
- A semantic LPIR op makes filetests/debug output obvious and lets the backend choose math-mode-specific lowering.
- The op must mean "float division by a compile-time constant," not "Q32 reciprocal algorithm."

### Q2: Should the frontend generate the op directly?

Decision:

- Yes. Have `lps-frontend` generate `FdivConstF32` for RHS constants that are cheap to detect while lowering `BinaryOperator::Divide`.

Why:

- `lower_binary_vec` already receives the right Naga expression handle before RHS is erased into a vreg.
- This catches the likely easy shader wins: literal divisors, module `const float`s, and simple scalar/vector constants.
- A later LPIR canonicalization pass can catch derived constants if profiles show this misses important cases.

### Q3: Should const-div lower as helper-equivalent reciprocal math or `lhs * q32(1 / rhs)`?

Decision:

- For the product fast path, lower as `lhs * q32(1.0 / rhs)`.

Why:

- Perf matters more than preserving helper edge semantics.
- This removes helper calls, runtime reciprocal division, runtime sign/abs divisor work, and special branchy helper behavior.
- It may behave differently for tiny divisors, saturation edges, and reciprocal values outside Q16.16 range. That is acceptable for normal shader rendering.
- Future math probes should be the correctness/debug story.

### Q4: What about constant zero divisors?

Decision:

- Keep `x / 0.0` off the multiply-by-recip path initially.

Why:

- `1.0 / 0.0` has no useful finite Q32 reciprocal.
- Constant-zero division is rare and edge-case-heavy.
- The backend can preserve existing helper behavior for zero constants until a debug/probe story exists.

### Q5: Should old Q32 math modes remain?

Decision:

- Treat the old modes as transitional. The product path should hardcode fast lowering.
- This plan should remove or bypass user-facing mode selection where it is cheap and safe.
- Do not spend significant implementation budget preserving saturating/reference mode plumbing.

Why:

- The product direction is fast-only render.
- Normal users should not pay complexity or branchiness for reference math.
- Probe-based diagnostics are planned later and should not be conflated with normal compile mode.

### Q6: Should wasm parity be included?

Decision:

- Yes at the semantic level. Add LPIR parse/format/validation support and at least a conservative wasm lowering for `FdivConstF32`.
- Native Q32 gets the performance-oriented lowering first.

Why:

- The host path should continue to compile shaders that use the new LPIR op.
- If wasm does not get the exact same speedup immediately, that is acceptable, but it must remain behaviorally usable.

## Suggested Implementation Direction

1. Add `LpirOp::FdivConstF32 { dst, lhs, rhs: f32 }`.
2. Update LPIR plumbing:
   - def/use handling
   - parser/printer/roundtrip tests
   - validator/type checks
3. Teach the frontend to emit `FdivConstF32` for RHS compile-time constants in float division.
4. Teach native Q32 lowering:
   - nonzero constant: precompute `q32(1.0 / rhs)` at compile time, materialize it, and reuse the existing inline wrapping Q32 multiply sequence
   - zero constant: fall back to existing dynamic `Fdiv` helper path by materializing RHS zero and calling existing lowering
   - F32 mode or non-Q32: lower conservatively
5. Collapse mode usage where practical:
   - Native normal `Fadd`/`Fsub`/`Fmul` should stay hardwired to inline fast path.
   - Native normal `Fdiv` should stay on reciprocal fast path or move to inline dynamic reciprocal in a later pass.
   - Model/compile-opt cleanup can happen in a separate phase if it starts to sprawl.

## Validation Notes

Add new focused filetests before or alongside implementation:

- literal positive const divisor
- literal negative const divisor
- fractional const divisor
- scalar divided by scalar const
- vector divided by scalar const
- vector divided by vector const
- `const float K = ...; x / K`
- constant zero divisor remains correct enough and does not try to emit multiply-by-infinite reciprocal
- dynamic divisor still compiles and keeps existing outputs

Useful focused command:

```sh
scripts/filetests.sh --target rv32n.q32 --detail \
  scalar/float/q32fast-div-recip.glsl \
  scalar/float/op-divide.glsl \
  scalar/float/q32fast-div-const.glsl
```

Final validation should include:

```sh
cargo fmt --all
cargo test -p lpir
cargo test -p lps-frontend
cargo test -p lpvm-native
cargo test -p lpvm-wasm
scripts/filetests.sh --target rv32n.q32 --detail \
  scalar/float/q32fast-div-recip.glsl \
  scalar/float/op-divide.glsl \
  scalar/float/q32fast-div-const.glsl
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```

If the implementation touches shader pipeline behavior broadly, also run:

```sh
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```
