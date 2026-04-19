# M4c — `lpfx-cpu` Migration

## Goal

Migrate `lpfx-cpu` (the CPU backend of the standalone `lpfx` effect
runner — used by `lpfx`-using tools and tests) onto `lp-shader`'s
high-level API: `LpsEngine::compile_px` for compilation, and
`LpsPxShader::render_frame` for rendering.

After M4c:

- `lpfx/lpfx-cpu/src/render_cranelift.rs` (~50 lines of hand-rolled
  pixel loop + Q32 → unorm16 conversion) is **deleted**.
- `lpfx/lpfx-cpu/src/compile.rs::compile_glsl` (ad-hoc
  `lps_frontend → LpvmEngine::compile`) is replaced by a thin
  `LpsEngine::compile_px` call. `validate_inputs` (manifest input ↔
  uniform check) stays — that's an `lpfx`-specific concern.
- `CpuFxInstance` holds an `LpsPxShader` instead of a raw
  `CraneliftState`.
- `lpfx::texture::CpuTexture` is either retired (if cleanly
  replaceable by `LpsTextureBuf`) or implements `TextureBuffer` so it
  can be used as a `render_frame` target.

## Why

`lpfx-cpu` and `lp-engine` ended up implementing the same per-pixel
loop, the same Q32 → unorm16 saturate-and-shift, and the same
direct-call wrapper around the Cranelift JIT. M4a removed the
duplication for `lp-engine`; M4c finishes the job for `lpfx-cpu`.

It also unblocks consolidation of texture types
(`lpfx::CpuTexture` ↔ `lp_shared::Texture` ↔ `LpsTextureBuf`) — three
implementations of "a buffer of pixels" exist today, which is one too
many.

## Why M4c after M4a (and probably after M4b)

- M4a hardens the `LpsPxShader` adapter shape against a real consumer
  before `lpfx-cpu` adopts it. Any rough edges in the API surface (per
  the open question in M4a, uniforms shim, texture-buffer adapter)
  will surface there first.
- M4b's backend swap, if it goes well, gives `lpfx-cpu` the option to
  pick the same Wasmtime-backed engine — same isolation/portability
  story.
- Order is **a → b → c** ideally, but M4c can run in parallel with M4b
  if needed (they touch different consumers).

## Deliverables

### `lpfx-cpu/src/lib.rs` rewrite

`CpuFxEngine` and `CpuFxInstance` use `LpsEngine` and `LpsPxShader`:

```rust
pub struct CpuFxEngine<E: LpvmEngine = WasmtimeEngine> {
    engine: LpsEngine<E>,
    textures: BTreeMap<TextureId, LpsTextureBuf>,
    next_id: u32,
}

pub struct CpuFxInstance {
    input_names: BTreeMap<String, String>,
    output: LpsTextureBuf,
    px: LpsPxShader,
}
```

The `<E: LpvmEngine = …>` default lets `lpfx-cpu` pick the same host
backend as `lp-engine` (Wasmtime once M4b lands; Cranelift today).
Defaulting allows downstream callers to keep their existing
`CpuFxEngine::new()` calls.

### `compile.rs` slimmed down

`compile_glsl` is replaced by a one-liner that calls
`LpsEngine::compile_px`. `CompiledEffect` may still be useful as a
public type but its `module` field becomes an `LpsPxShader`.

`validate_inputs` stays as-is — it operates on the `LpsModuleSig`
which is still surfaced by `LpsPxShader::meta()` (verify the accessor
exists; add if not).

### `render_cranelift.rs` deleted

The whole file goes away. `CpuFxInstance::render(time)` becomes:

```rust
fn render(&mut self, time: f32) -> Result<(), String> {
    let uniforms = LpsValueF32::struct_(&[
        ("outputSize", vec2(self.output.width(), self.output.height())),
        ("time", time.into()),
        // plus manifest-driven inputs from input_names
    ]);
    self.px.render_frame(&uniforms, &mut self.output)
        .map_err(|e| e.to_string())
}
```

Same open question as M4a applies: how does `LpsPxShader` see the
legacy `render(fragCoord, outputSize, time)` signature? Whatever
answer M4a picked, M4c uses it identically.

### Texture consolidation

Three options for `lpfx::texture::CpuTexture`:

1. **Replace entirely with `LpsTextureBuf`.** Cleanest. Touches every
   `lpfx-cpu` user that names `CpuTexture` directly. Recommended if
   the surface area is small (likely).
2. **Implement `TextureBuffer` on `CpuTexture`.** Lets `render_frame`
   write into it without copying, but keeps two type names alive.
3. **Adapter struct that wraps `LpsTextureBuf` and exposes the
   `CpuTexture` API.** Worst of both worlds; only useful if external
   API stability is critical (it isn't here).

Recommendation: **option 1**, with a brief deprecation alias if any
public API needs to stay green for one release.

### Cargo features

`lpfx-cpu` currently has a `cranelift` feature flag. Same renaming
question as M4b. Recommendation: rename to whatever M4b picked; keep
the names consistent across consumers.

## Validation

```bash
cargo test -p lpfx -p lpfx-cpu
# Verify: noise.fx still renders correctly
# Verify: any other lpfx-using examples/binaries still work
```

End-to-end:

- `noise.fx` (or whatever the canonical `lpfx-cpu` example is) renders
  pixel-identically to the pre-M4c baseline.
- Manifest input validation still rejects mismatches (existing tests
  pass without modification).

## Risks

- **External API churn.** `lpfx-cpu` is consumed by tools outside the
  workspace (or at least, that's the historical norm for `lpfx-*`).
  Renaming `CpuTexture` and re-typing `CpuFxInstance` may break
  downstream code. Mitigate with type aliases for one release if
  needed.
- **`CpuTexture` parity.** Verify `LpsTextureBuf` exposes everything
  `CpuTexture` does (set/get pixels, raw access, format conversions).
  Add accessors if missing.

## Dependencies

- M2 — done
- M4a (strongly preferred predecessor; same API consumer pattern)
- M4b — optional but lets `lpfx-cpu` ride the same backend swap

## Out of scope

- `lpfx-gpu` (wgpu backend) — separate path, not affected by this
  migration.
- Removing `lpvm-cranelift` from the workspace.
- M1 fragment-contract migration of `noise.fx` (separate milestone).
