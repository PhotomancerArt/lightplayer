# Phase 1: Fix psrdnoise seed parameter bug

## Scope of phase

Add `seed: UInt` to the `lpfx_psrdnoise` GLSL signatures (vec2 and vec3 overloads). Fix the
Cranelift builtin registry to declare the correct parameter count. Update all GLSL shader sources.
Regenerate derived code.

This is a cross-cutting fix: it touches frontend (GLSL signatures), Cranelift (registry), generated
code (builtin mapping, WASM import types), and shader sources. It must land first because subsequent
phases depend on the GLSL signature being correct.

## Code organization reminders

- Generated files (`glsl_builtin_mapping.rs`, `builtin_wasm_import_types.rs`, `builtin_refs.rs`,
  etc.) must be regenerated via `scripts/build-builtins.sh` or
  `cargo run -p lp-glsl-builtins-gen-app` — do not hand-edit.
- Prefer running the generator once after all source-of-truth changes are made.

## Implementation details

### 1. Update GLSL signatures in `lpfx_fns.rs`

File: `lp-shader/lp-glsl-frontend/src/semantic/lpfx/lpfx_fns.rs`

Add `seed: UInt` as the last `In` parameter for both `lpfx_psrdnoise` overloads (vec2 at ~line 299
and vec3 at ~line 331):

```rust
ParameterRef {
    name: "seed",
    ty: Type::UInt,
    qualifier: ParamQualifier::In,
},
```

The vec2 overload goes from 4 params `[vec2, vec2, float, out vec2]` to 5 params
`[vec2, vec2, float, out vec2, uint]`. Same pattern for vec3.

### 2. Fix Cranelift registry

File: `lp-shader/lp-glsl-cranelift/src/backend/builtins/registry.rs`

`signature_for_builtin` for `LpfxPsrdnoise2F32 | LpfxPsrdnoise2Q32` currently has 6 params (5× i32 +
pointer). Add the seed param (i32) to make it 7:

```rust
BuiltinId::LpfxPsrdnoise2F32 | BuiltinId::LpfxPsrdnoise2Q32 => {
    sig.params.push(AbiParam::new(types::I32)); // x
    sig.params.push(AbiParam::new(types::I32)); // y
    sig.params.push(AbiParam::new(types::I32)); // period_x
    sig.params.push(AbiParam::new(types::I32)); // period_y
    sig.params.push(AbiParam::new(types::I32)); // alpha
    sig.params.push(AbiParam::new(pointer_type)); // gradient_out
    sig.params.push(AbiParam::new(types::I32)); // seed
    sig.returns.push(AbiParam::new(types::I32));
}
```

Same for `LpfxPsrdnoise3F32 | LpfxPsrdnoise3Q32` — add one more i32 param for seed.

### 3. Update shader sources

Two files use `lpfx_psrdnoise`:

- `examples/basic/src/rainbow.shader/main.glsl`
- `examples/mem-profile/src/rainbow.shader/main.glsl`

Change the call from:

```glsl
float noiseValue = lpfx_psrdnoise(
    scaledCoord,
    vec2(0.0),
    time,
    gradient
);
```

To:

```glsl
float noiseValue = lpfx_psrdnoise(
    scaledCoord,
    vec2(0.0),
    time,
    gradient,
    0u
);
```

### 4. Regenerate

```bash
cargo run -p lp-glsl-builtins-gen-app
```

This updates:

- `lp-glsl-builtin-ids/src/glsl_builtin_mapping.rs` — psrdnoise param kinds gain
  `GlslParamKind::UInt`
- Other generated files should be unaffected (WASM import types already encode the full extern "C"
  ABI which includes seed)

### 5. Verify no Cranelift test breakage

psrdnoise doesn't appear to have dedicated Cranelift tests, but run the full suite to catch
regressions in overload resolution or the LPFX call path.

## Validate

```bash
cargo run -p lp-glsl-builtins-gen-app
cd lp-glsl && cargo test -p lp-glsl-frontend
cd lp-glsl && cargo test -p lp-glsl-cranelift
cd lp-glsl && cargo test -p lp-glsl-builtin-ids
cd lp-glsl && cargo test -p lp-glsl-wasm
cargo +nightly fmt
```

Fix any warnings introduced. The `glsl_builtin_mapping.rs` match arms for psrdnoise should now have
5 param kinds instead of 4.
