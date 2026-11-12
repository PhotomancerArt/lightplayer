# M4a — Pixel-Loop Migration

> **Status: ✅ complete.** Implementation plan:
> [`docs/plans-old/2026-04-19-m4a-pixel-loop-migration.md`](../../plans-old/2026-04-19-m4a-pixel-loop-migration.md)
> (see plan-old; deliverables ticked below).

## Goal

Move the per-pixel render loop out of `lp-engine`'s graphics backends and
into `lp-shader::LpsPxShader::render_frame` (which itself delegates to the
synthesised `__render_texture_<format>` LPIR function from M2).

Both live host paths get the same treatment, **without changing which LPVM
backend they use**:

- `lp-core/lp-engine/src/gfx/cranelift.rs::render_direct_call` — desktop /
  `fw-emu`, currently `lpvm-cranelift`.
- `lp-core/lp-engine/src/gfx/native_jit.rs::render_native_jit_direct` —
  ESP32 firmware, `lpvm-native` (RV32fa JIT).

After M4a, the per-pixel `for y in 0..h { for x in 0..w { … } }` loops,
the per-channel Q32 → unorm16 conversion, and the `set_pixel_u16` calls
are gone from `lp-engine/src/gfx/`. The backend wrappers become thin
adapters: compile via `LpsEngine::compile_px`, render via
`LpsPxShader::render_frame`.

The host backend swap (Cranelift → Wasmtime) is **out of scope here**;
that's M4b. M4a keeps each gfx file on its current backend so the
correctness change and the backend-choice change can be reviewed and
bisected independently.

## Why M4a first

- **Cruft removal that lands today.** Two near-duplicate hand-rolled
  pixel loops (~50 lines each, identical math) collapse into one call
  site. The same simplification holds whether or not we ever swap to
  Wasmtime.
- **Unblocks M4b mechanically.** Once `LpShader::render` is just
  `lps_px_shader.render_frame(uniforms, tex)`, swapping the underlying
  `LpvmEngine` is a one-line change to the gfx wrapper.
- **Unblocks M4c symmetrically.** `lpfx-cpu` does the same migration
  against the same API; small, isolated.
- **Surfaces the `render`-signature question early** (see open question
  below) on the simplest customer.

## Deliverables

> All deliverables below are ✅ complete; see plan-old for details and
> the `# Decisions for future reference` section therein for any
> deviations from the original recipe.

### `lp-engine/src/gfx/cranelift.rs` rewrite

Replace `CraneliftShader` and `render_direct_call` with an adapter that
holds an `LpsPxShader`:

```rust
use lp_shader::{LpsEngine, LpsPxShader, LpsTextureBuf, LpsValueF32, TextureStorageFormat};
use lpvm_cranelift::{CompileOptions, CraneliftEngine, ...};

pub struct CraneliftGraphics {
    engine: LpsEngine<CraneliftEngine>,
}

impl LpGraphics for CraneliftGraphics {
    fn compile_shader(&self, source: &str, options: &ShaderCompileOptions)
        -> Result<Box<dyn LpShader>, Error>
    {
        // (engine config still threads CompilerConfig::q32 from options)
        let px = self.engine.compile_px(source, TextureStorageFormat::Rgba16Unorm)?;
        Ok(Box::new(CraneliftShader { px }))
    }
}

struct CraneliftShader { px: LpsPxShader }

impl LpShader for CraneliftShader {
    fn render(&mut self, texture: &mut Texture, time: f32) -> Result<(), Error> {
        // texture/uniform glue; see "Texture-buffer adapter" below
        self.px.render_frame(&uniforms, &mut tex_buf)
    }
}
```

`CraneliftEngine::new(opts)` is constructed once and lives inside
`LpsEngine` for the lifetime of `CraneliftGraphics` instead of being
rebuilt per-`compile_shader` call. (Today's code re-constructs it each
time.)

### `lp-engine/src/gfx/native_jit.rs` rewrite

Same shape, swapping `CraneliftEngine` for whatever `LpvmEngine` impl
`lpvm-native` exposes (the existing `NativeJitEngine` wrapped or
re-exposed as an `LpvmEngine`). `render_native_jit_direct` and the inner
`for y/x` loop are deleted. `BuiltinTable` ownership stays where it is
today (likely inside `NativeJitGraphics::new`); we'll just feed it into
the engine constructor used by `LpsEngine`.

### Texture-buffer adapter

The engine renders into `lp_shared::Texture` (the existing engine-side
texture). `LpsPxShader::render_frame` writes into `LpvmBuffer` /
`LpsTextureBuf` (LPVM-managed shared-memory buffer).

We have two options; pick one in implementation:

- **A. Allocate an `LpsTextureBuf` per shader instance**, render into
  it, then `memcpy` into the engine `Texture`. Simple, one extra copy
  per frame. Acceptable for M4a; revisit in M4b/c if it shows up.
- **B. Make `lp_shared::Texture` *be* an `LpsTextureBuf`** (or
  implement `TextureBuffer` over its existing storage). Zero-copy but
  pulls in shared-memory allocation lifetime concerns into engine
  texture management.

Recommendation: **A for M4a** (lowest risk, easiest to revert). M4b/c
or a follow-up consolidation milestone can take on B.

### Uniforms shim

`LpsPxShader::render_frame(uniforms, tex)` takes an `LpsValueF32`
struct describing all uniforms. Today the engine passes `time` as a
function argument and computes `outputSize` from texture dims. The
adapter constructs an `LpsValueF32` per-frame from those values
(`outputSize = vec2(w, h)`, `time = f32`).

This is straightforward but depends on the open question below.

### Verify hot-path overhead

Once both gfx files are migrated, run `lp-cli shader-debug` and a brief
perf check: render a constant-color shader for N frames on `fw-emu` and
on the bench host, confirm we haven't regressed against the
hand-rolled loop. Document the result in
`docs/design/native/perf-report/`.

## Open question (blocker — please decide before implementation)

`lp-engine` shaders today use the legacy signature:

```glsl
vec4 render(vec2 fragCoord, vec2 outputSize, float time);
```

`LpsEngine::compile_px` (M2) expects:

```glsl
vec4 render(vec2 pos);
```

`outputSize` and `time` are supposed to come in as **uniforms** in the
new world (set per-frame via `render_frame`).

Three ways to bridge:

1. **Bootstrap-wrapper inside `compile_px`.** Detect the legacy 3-arg
   `render`, wrap it with synthetic GLSL:
   ```glsl
   uniform vec2 outputSize;
   uniform float time;
   vec4 __px_render(vec2 pos) { return render(pos, outputSize, time); }
   ```
   and treat `__px_render` as the entry. **Pros:** existing shaders +
   docs unchanged. **Cons:** mild magic in the front-end; `compile_px`
   gets a small text-substitution path.
2. **Extend `compile_px` to accept the legacy 3-arg signature
   directly** by making `outputSize`/`time` synthesised uniforms inside
   `LpsPxShader` rather than user-visible. **Pros:** no GLSL rewrite.
   **Cons:** `LpsPxShader` knows about engine-specific naming
   conventions; couples the layer.
3. **Migrate engine shaders to the M1 fragment contract first** (`out
   vec4 fragColor; void main()` with all dynamic state via uniforms).
   **Pros:** zero coupling, future-proof. **Cons:** M1 hasn't shipped;
   bigger effort; touches user-visible shader files (rainbow.glsl,
   etc.).

Recommendation: **option 1 for M4a/c**, with option 3 as a clean-up
follow-up after M1 ships. The wrapper is ~10 lines of synthesised GLSL
and lives entirely inside `lp-shader`.

If the answer changes, the plan files spawned from M4a will need a
small tweak; the overall structure is unaffected.

## Validation

```bash
cargo test -p lp-engine
cargo build --features cranelift -p fw-emu
just build-rv32   # confirm fw-esp32 still builds with native-jit
```

End-to-end:

- `fw-emu` renders rainbow.glsl correctly (visual diff against
  pre-M4a baseline; pixel-exact preferred).
- `fw-esp32` (when flashed) renders rainbow.glsl correctly; FPS within
  noise of the M2 baseline.
- `info!` log line `Shader N q32 options: …` still appears (M4a should
  not regress the q32-options dispatch wiring).

## Dependencies

- M2 (`render_frame` via synthetic `__render_texture`) — done
- Resolution of the open question above

## Out of scope

- Cranelift → Wasmtime backend swap (M4b)
- `lpfx-cpu` migration (M4c)
- M1 fragment contract migration (separate roadmap milestone)
- Removing `lpvm-cranelift` from the workspace (separate, post-M4b)
- Texture-buffer unification (option B above) — revisit if option A
  performance is a problem
