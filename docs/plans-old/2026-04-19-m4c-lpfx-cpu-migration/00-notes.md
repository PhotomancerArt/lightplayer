# M4c — `lpfx-cpu` Migration

Roadmap milestone:
[`docs/roadmaps/2026-04-16-lp-shader-textures/m4c-lpfx-cpu-migration.md`](../../roadmaps/2026-04-16-lp-shader-textures/m4c-lpfx-cpu-migration.md)

Predecessor (in flight in this working tree):
[`docs/plans/2026-04-19-m4b-host-backend-swap/`](../2026-04-19-m4b-host-backend-swap/)

## Scope of work

Migrate `lpfx-cpu` (the standalone `lpfx` effect runner's CPU
backend) onto `lp-shader`'s high-level API, mirroring what M4a did
for `lp-engine`:

1. **Compile pipeline.** Replace `compile.rs::compile_glsl`'s ad-hoc
   `lps_frontend → LpvmEngine::compile` wiring with a one-line call
   to `LpsEngine::compile_px`. Keep `compile.rs::validate_inputs`
   (manifest input ↔ uniform-field shape check) — that's an
   `lpfx`-specific concern.
2. **Render pipeline.** Delete `render_cranelift.rs` (the ~50-line
   hand-rolled per-pixel loop + Q32 → unorm16 conversion). Render
   becomes a single call to `LpsPxShader::render_frame(&uniforms,
   &mut output)`.
3. **Backend selection.** Drop the `cranelift` Cargo feature. Adopt
   the same `cfg(target_arch = …)` dispatch M4b establishes for
   `lp-engine::Graphics` (RV32 → `lpvm-native`, wasm32 →
   `lpvm-wasm` browser, catchall → `lpvm-wasm` wasmtime). Single
   unqualified `CpuFxEngine` type per target.
4. **Texture consolidation.** Retire `lpfx::texture::CpuTexture` in
   favour of `LpsTextureBuf` (roadmap option 1). `FxInstance::output`
   returns an `LpsTextureBuf` (or `&dyn TextureBuffer`).
5. **`noise.fx` shader migration.** M4a explicitly deferred
   `noise.fx`'s conversion from the legacy `vec4 render(vec2
   fragCoord, vec2 outputSize, float time)` form to the new
   `render(vec2 pos)` + uniforms form (per
   [`docs/plans-old/2026-04-19-m4a-pixel-loop-migration.md`](../../plans-old/2026-04-19-m4a-pixel-loop-migration.md):
   `# noise.fx/main.glsl deferred to M4c`). `LpsEngine::compile_px`
   enforces the new contract via `validate_render_sig`, so this
   conversion has to happen in M4c (or M4c has to inject a GLSL
   bootstrap wrapper — see Q1).

After M4c the workspace's only remaining `lpvm-cranelift` consumers
are `lp-cli shader-debug` (AOT object generation, separate path) and
the in-tree `lpvm-cranelift` smoke tests. Removing the crate from
the workspace entirely is **not** in scope (separate later task per
the overview).

Out of scope:

- `lpfx-gpu` (wgpu backend) — separate path, unaffected.
- Removing `lpvm-cranelift` from the workspace.
- Wasmtime perf tuning / lpfx-cpu performance benchmarking.
- M3 — texture reads (`texelFetch`, `sampler2D`).
- Re-shaping the `lpfx::FxValue` taxonomy (Color, Palette, etc. as
  separate variants from F32/I32/Bool/Vec3) — current variants
  cover all `noise.fx` inputs.

## Current state of the codebase

### `lpfx/lpfx-cpu/Cargo.toml`

```toml
[features]
default = ["cranelift"]
cranelift = ["dep:lpvm-cranelift"]

[dependencies]
lpfx = { path = "../lpfx" }
lpir = { path = "../../lp-shader/lpir" }
lpvm = { path = "../../lp-shader/lpvm", default-features = false }
lps-frontend = { path = "../../lp-shader/lps-frontend" }
lps-shared = { path = "../../lp-shader/lps-shared", default-features = false }

lpvm-cranelift = { path = "../../lp-shader/lpvm-cranelift", optional = true, default-features = false, features = ["std", "glsl"] }
```

`#![no_std]` at the top of `src/lib.rs`. `extern crate alloc;`.

### `lpfx/lpfx-cpu/src/lib.rs`

- `CpuFxEngine { textures: BTreeMap<TextureId, CpuTexture>, next_id: u32 }`
  — texture pool keyed by `lpfx::TextureId`.
- `#[cfg(feature = "cranelift")] CpuFxInstance { input_names:
  BTreeMap<String, String>, output: CpuTexture, cranelift:
  CraneliftState }` — owns the texture (moved out of the engine
  pool at instantiate time) plus the JIT module/instance/direct-call.
- `instantiate` constructs a fresh `CraneliftEngine` per instance
  (no engine reuse), removes the texture from the pool, calls
  `compile::compile_glsl`, calls `compile::validate_inputs`,
  builds a `direct_call("render")`, applies manifest defaults via
  `set_input` calls.
- `set_input(name, FxValue)` translates `FxValue → LpsValueF32` and
  calls `instance.set_uniform(uniform_name, &lps_val)` — i.e.
  uniforms are mutated in-place on the `LpvmInstance`. Mappings:
  `F32→F32`, `I32→I32`, `Bool→Bool`, `Vec3→Vec3`. (`Color` and
  `Palette` `FxInputType`s have no `FxValue` representation today.)
- `render(time)` calls `render_cranelift` which loops every pixel,
  resets globals, calls `direct_call.call_i32_buf(vmctx, args, &mut
  rgba_q32)` with `[fragX_q32, fragY_q32, outX_q32, outY_q32,
  time_q32]`, clamps Q32 → unorm16, writes to `CpuTexture` via
  `set_pixel_u16`.

### `lpfx/lpfx-cpu/src/compile.rs`

- `compile_glsl<E: LpvmEngine>(engine, glsl)`: parse → lower →
  `engine.compile(&ir, &meta)` (no `compile_with_config`; per-call
  config lost). Returns `CompiledEffect { module, meta, _ir }`.
- `validate_inputs(manifest, meta)`: for each manifest input `X`,
  asserts `meta.uniforms_type` resolves the path `input_X`. Pure
  metadata check — stays as-is in M4c.

### `lpfx/lpfx-cpu/src/render_cranelift.rs`

The hand-rolled per-pixel loop and Q32 → unorm16 conversion that
M4a moved out of `lp-engine`'s gfx backends. ~50 lines.
**Deleted in M4c.**

### `lpfx/lpfx/src/texture.rs`

`pub struct CpuTexture { width, height, format, data: Vec<u8> }`
with `set_pixel_u16` / `pixel_u16` / `data` / `data_mut` / `width`
/ `height` / `format`. `pub enum TextureFormat { Rgba16 }` (sole
variant). `pub struct TextureId(u32)`.

`CpuTexture` is re-exported from the `lpfx` crate root. `TextureId`
and `TextureFormat` too.

### `lpfx/lpfx/src/engine.rs`

```rust
pub trait FxEngine {
    type Instance: FxInstance;
    type Error: core::fmt::Display;
    fn create_texture(&mut self, w: u32, h: u32, format: TextureFormat) -> TextureId;
    fn instantiate(&mut self, module: &FxModule, output: TextureId)
        -> Result<Self::Instance, Self::Error>;
}

pub trait FxInstance {
    type Error: core::fmt::Display;
    fn set_input(&mut self, name: &str, value: FxValue) -> Result<(), Self::Error>;
    fn render(&mut self, time: f32) -> Result<(), Self::Error>;
}
```

These traits don't surface `CpuTexture` directly — only via
`TextureFormat` and the opaque `TextureId`. The `output()` accessor
on `CpuFxInstance` is concrete (returns `&CpuTexture`) and is
**not** part of `FxInstance`.

### `examples/noise.fx/main.glsl`

```glsl
layout(binding = 0) uniform float input_speed;
…  // 6 input_* uniforms total

vec4 render(vec2 fragCoord, vec2 outputSize, float time) {
    float t = time * input_speed;
    …
}
```

Legacy 3-arg signature. `LpsEngine::compile_px`'s
`validate_render_sig` requires exactly one `vec2` parameter and
returns `LpsError::Validation` otherwise. So `noise.fx` cannot
compile through `compile_px` until either it migrates or
`compile_px` accepts the legacy form.

### `lp-shader/lp-shader` (the API M4c adopts)

Today (post-M4a) provides:

- `LpsEngine<E: LpvmEngine>::new(engine)`.
- `LpsEngine::compile_px(glsl, output_format, &CompilerConfig) ->
  Result<LpsPxShader, LpsError>` — synthesises
  `__render_texture_<format>` and validates `render(vec2 pos)`.
- `LpsEngine::alloc_texture(w, h, format) ->
  Result<LpsTextureBuf, AllocError>` — guest-addressable shared
  buffer.
- `LpsPxShader::render_frame(&uniforms, &mut LpsTextureBuf)` — the
  per-pixel loop and format-aware writes happen inside the
  synthesised function. Uniforms are passed as
  `LpsValueF32::Struct` and applied each frame; there is **no**
  public per-uniform setter on `LpsPxShader` (the existing
  `apply_uniforms` is private).
- `LpsPxShader::meta() -> &LpsModuleSig` — exposes
  `uniforms_type` for `validate_inputs`.

### M4b end-state assumed by M4c

- `lpvm-wasm`'s `WasmLpvmEngine` will have a working
  `compile_with_config` override (M4b phase 1) so per-call
  `CompilerConfig` reaches the WASM emitter.
- `WasmtimeLpvmMemory` will pre-grow its linear memory once (M4b
  phase 1) so `LpvmBuffer.native` pointers stay valid for the
  lifetime of an `LpsTextureBuf`.
- `lp-engine::Graphics` becomes a single unqualified type with
  target-arch dispatch and no Cargo feature for backend selection
  (M4b phase 2). `lp-engine` no longer depends on `lpvm-cranelift`.
- `backend_name()` strings are `"lpvm-native::rt_jit"`,
  `"lpvm-wasm::rt_wasmtime"`, `"lpvm-wasm::rt_browser"`.

If M4b lands with material deviations from this end-state, the M4c
phase files may need a small re-edit before sub-agent dispatch.

### Consumers of `lpfx-cpu`

`grep -r 'lpfx_cpu\|lpfx-cpu' Cargo.toml '**/Cargo.toml'` shows
`lpfx-cpu` is referenced only by:

- The workspace `Cargo.toml`'s `members` and
  `default-members` lists.
- Its own `Cargo.toml`.

No other crate depends on `lpfx-cpu`. So unlike M4b, M4c has no
"consumer fan-out" phase — the blast radius is contained to
`lpfx/lpfx/` and `lpfx/lpfx-cpu/`. External (out-of-tree) consumers
may exist; treat them as out of scope for this plan.

## Questions

### Q1. `noise.fx` legacy render signature — migrate or wrap? ✅ resolved

**Answer.** **A — migrate `noise.fx` in M4c.** Edit
`examples/noise.fx/main.glsl`: declare `outputSize` and `time` as
`layout(binding = …) uniform …` (mirror engine shaders), rename
`render(vec2 fragCoord, vec2 outputSize, float time)` →
`render(vec2 pos)`, rename `fragCoord` → `pos` inside the body.
No semantic change. One-file edit. Matches M4a precedent for every
other shader in the tree; keeps a single render contract across
workspace. No bootstrap wrapper introduced.

**Context.** `noise.fx` uses
`vec4 render(vec2 fragCoord, vec2 outputSize, float time)`.
`LpsEngine::compile_px`'s `validate_render_sig` requires exactly
one `vec2` parameter. The M4a plan explicitly says
`# noise.fx/main.glsl deferred to M4c`, and the roadmap notes
"Whatever answer M4a picked, M4c uses it identically" — but M4a
just migrated each engine shader to the new form (no wrapper
generated), see e.g. `examples/basic/src/rainbow.shader/main.glsl`
which now uses
`layout(binding = 0) uniform vec2 outputSize; … vec4 render(vec2 pos)`.

The roadmap overview decision 3 also describes a **bootstrap
wrapper option** for consumers that want to keep the legacy
user-facing API:

```glsl
uniform vec2 outputSize;
uniform float time;
out vec4 fragColor;
// ... user code with render() definition ...
void main() { fragColor = render(gl_FragCoord.xy, outputSize, time); }
```

**Options.**

A. **Migrate `noise.fx` in M4c.** Edit
   `examples/noise.fx/main.glsl` to declare `outputSize` and `time`
   as uniforms and change `render(fragCoord, outputSize, time)` →
   `render(vec2 pos)` (using `pos` for what was `fragCoord`).
   Mirrors the M4a approach for engine shaders. One-file change.
   Trades one user-visible legacy contract for the new uniform
   contract; lpfx no longer documents the 3-arg form.

B. **Inject a GLSL bootstrap wrapper in `lpfx-cpu`.** Before
   handing GLSL to `LpsEngine::compile_px`, prepend
   `uniform vec2 outputSize; uniform float time;` and append a
   `vec4 render(vec2 pos) { return render(pos, outputSize, time); }`
   shim that calls the user's `render`. Keeps the legacy contract
   alive end-to-end. Requires the user's GLSL to not already have
   colliding identifiers; introduces a hidden contract on what
   names lpfx-cpu adds.

C. **Loosen `validate_render_sig` to accept both shapes.** Detect
   the 3-arg form, internally generate `render(vec2 pos)` that
   delegates. Pushes the wrapping into `lp-shader` itself. Spreads
   the lpfx-specific contract into a general crate; rejected on
   layering grounds.

**Suggested answer.** **A — migrate `noise.fx` in M4c.** Matches
what M4a did for engine shaders; one-line GLSL edit; keeps `lpfx`
and `lp-shader` honest about a single render contract; nothing in
the lpfx documentation today commits to the legacy form. The
roadmap's "M1 fragment-contract migration of noise.fx" note refers
to the `lpfx`-public contract (FxModule, manifest, etc.); the GLSL
shape inside the example file is incidental. If a future external
consumer wants the legacy 3-arg form, the bootstrap wrapper from
option B can be added then as an explicit feature, not as a
silent default.

### Q2. Backend selection — target-arch (mirror M4b) or generic? ✅ resolved

**Answer.** **A — pure target-arch dispatch.** `CpuFxEngine` is a
single concrete (non-generic) type per target. `Cargo.toml` uses
`[target.'cfg(target_arch = "riscv32")'.dependencies]` for
`lpvm-native` and `[target.'cfg(not(target_arch = "riscv32"))'.dependencies]`
for `lpvm-wasm`. No `cranelift` / `wasmtime` Cargo feature on
`lpfx-cpu`. Module layout under `src/` mirrors `lp-engine/src/gfx/`'s
target-arch dispatch (host / wasm_guest / native_jit). The
roadmap's `CpuFxEngine<E = WasmtimeEngine>` shape is treated as a
pre-M4b-Q2.5 draft, not binding. If a downstream caller later
needs a custom `LpvmEngine`, the generic escape hatch is additive.

**Context.** Roadmap deliverable shows:

```rust
pub struct CpuFxEngine<E: LpvmEngine = WasmtimeEngine> { engine: LpsEngine<E>, … }
```

Generic with default. M4b's notes (Q2.5 wrap-up) say: "User
confirmed lpfx-cpu is intended to do basically the same thing — i.e.
adopt the same target-arch-based selection pattern." The two are in
tension.

**Options.**

A. **Pure target-arch dispatch, mirror M4b exactly.** Drop the
   generic. `CpuFxEngine` is a single concrete type per target,
   selected by `cfg(target_arch = …)` blocks in `Cargo.toml` and
   `src/`. Same shape as `lp_engine::Graphics`. Simple.
   Downstream callers cannot pick a different backend on a given
   target; opening that escape hatch later is additive.

B. **Generic `CpuFxEngine<E>` with target-defaulted `E`.** Each
   target gets a default backend type alias
   (`pub type DefaultLpvmEngine = WasmLpvmEngine` on host, etc.),
   `CpuFxEngine<E = DefaultLpvmEngine>`. Downstream callers can
   instantiate with any `LpvmEngine`. More flexible; adds a
   non-trivial type parameter to the public API; the default makes
   the common path identical to A.

C. **Cargo feature, like today's `cranelift` flag, but renamed.**
   Rejected — directly contradicts both the M4b model and the
   user's stated direction.

**Suggested answer.** **A — pure target-arch dispatch.** Same
rationale as M4b Q2.5: one backend per target, no feature
matrix, single concrete `CpuFxEngine`. If a downstream consumer
later needs to swap (e.g. a host tool wanting native-jit for some
reason), add a generic escape hatch then; it's an additive change
that doesn't disturb the simple default. Diverging from M4b
without a concrete need would just create two patterns to maintain.

### Q3. `CpuTexture` retirement — replace, alias, or keep alongside? ✅ resolved

**Answer.** **A — drop `CpuTexture` and `TextureFormat`, expose
`LpsTextureBuf` end-to-end.** Texture storage lives in `lp-shader`;
keeping a parallel `lpfx::texture::CpuTexture` undermines the whole
point of the migration.

- `lpfx::texture::CpuTexture` and `lpfx::texture::TextureFormat`
  deleted (along with their crate-root re-exports).
- `lpfx::texture` shrinks to just `pub struct TextureId(u32)` —
  the engine-side opaque handle into the texture pool.
- `FxEngine::create_texture` loses its `format` parameter
  (`Rgba16Unorm`-only on CPU today per overview decision 5; when a
  second format is needed, surface `lps_shared::TextureStorageFormat`
  directly rather than maintaining a parallel taxonomy).
- `CpuFxInstance::output()` returns `&LpsTextureBuf` (concrete,
  not `&dyn TextureBuffer` — callers can upcast where they want
  the trait).
- Tests read pixels via `TextureBuffer::data()`, with a tiny
  `#[cfg(test)]` `pixel_u16(buf, x, y)` helper inside `lpfx-cpu`'s
  test module for readability.

**Context.** Roadmap recommends option 1 (replace `CpuTexture`
entirely with `LpsTextureBuf`). Affected surface:

- `lpfx::texture::CpuTexture` (the type itself).
- `lpfx::texture::TextureFormat` (single-variant enum `Rgba16`,
  parallel to `lps_shared::TextureStorageFormat::Rgba16Unorm`).
- `lpfx::texture::TextureId` (opaque u32 handle, no shared-memory
  notion — issued by `FxEngine::create_texture`).
- Re-exports from `lpfx` crate root.
- `FxEngine::create_texture(w, h, TextureFormat) -> TextureId` and
  the `output: CpuTexture` field on `CpuFxInstance`.
- The `instance.output() -> &CpuTexture` accessor used by
  `lpfx-cpu`'s tests for pixel readback (`output.pixel_u16(x, y)`).

**Options.**

A. **Drop `CpuTexture` and `TextureFormat`. Use `LpsTextureBuf`
   end-to-end.**
   - `FxEngine::create_texture(w, h)` (no format param — only
     `Rgba16Unorm` is supported on the CPU path today, see overview
     decision 5).
   - `CpuFxInstance::output()` returns `&LpsTextureBuf` (or
     `&dyn lps_shared::TextureBuffer` for trait-object friendliness).
   - Tests read pixels via `TextureBuffer::data()` (raw `&[u8]`)
     instead of `pixel_u16(x, y)`. Slight churn in the test code.
   - `TextureId` stays as the engine's opaque handle into the
     `BTreeMap<TextureId, LpsTextureBuf>` pool.
   - Cleanest end state.

B. **Keep `TextureFormat` (rename to alias / drop variants),
   replace `CpuTexture` storage with `LpsTextureBuf`.** Two name
   roots for "format" (one in lpfx, one in lps-shared) with a 1:1
   mapping. Adds a translation step for no obvious gain.

C. **Keep `CpuTexture` as a wrapper over `LpsTextureBuf`.** Worst
   of both worlds per roadmap.

D. **Keep `CpuTexture` alongside `LpsTextureBuf` and copy at
   render boundary.** Doubles per-frame memory and copies pixels
   for nothing.

**Suggested answer.** **A — drop `CpuTexture` and `TextureFormat`,
expose `LpsTextureBuf` directly.** Removes the duplicate type at
the source; matches the overview's "consolidate texture types"
motivation. `TextureId` stays as the opaque handle into the
engine's texture pool — that's just an `lpfx`-side allocator
detail, not a duplicate type. Keep `pixel_u16(x, y)`-style
helpers on the test side as small free functions if needed (or
leave as direct `TextureBuffer::data()` byte indexing).

If external consumers need API compatibility, a thin
`#[deprecated]` re-export shim can be added later — not in this
plan.

### Q4. `no_std` — keep, drop, or target-gate? ✅ resolved

**Answer.** **Keep `#![no_std]`; thread `std` through the dep
tree exactly like `lp-engine` does.** Reason: `lp-engine` will
soon consume `lpfx` (and by extension `lpfx-cpu`), so `lpfx-cpu`
must remain RV32 / firmware-buildable.

Concrete shape, mirroring `lp-core/lp-engine/Cargo.toml`:

```toml
# lpfx/lpfx-cpu/Cargo.toml
[features]
default = ["std"]
std = [
    "lpfx/std",            # forward to parent
    "lp-shader/std",
    "lps-shared/std",
    # plus any other downstream `std` knobs the dep tree exposes
]

[dependencies]
lpfx       = { path = "../lpfx",                          default-features = false }
lp-shader  = { path = "../../lp-shader/lp-shader",        default-features = false }
lps-shared = { path = "../../lp-shader/lps-shared",       default-features = false }
lpvm       = { path = "../../lp-shader/lpvm",             default-features = false }
lpir       = { path = "../../lp-shader/lpir" }

[target.'cfg(target_arch = "riscv32")'.dependencies]
lpvm-native = { path = "../../lp-shader/lpvm-native", default-features = false }

[target.'cfg(not(target_arch = "riscv32"))'.dependencies]
lpvm-wasm = { path = "../../lp-shader/lpvm-wasm", default-features = false }
```

`lpfx-cpu/src/lib.rs` keeps `#![no_std]` + `extern crate alloc`.
All `lpfx-cpu` code uses only `core` + `alloc` types. The
`lpvm-wasm` dep is std-internally but that's transparent to
lpfx-cpu's own attribute (same as `lp-engine` today).

If `lpfx`'s parent crate doesn't yet expose a `std` feature for
forwarding, add a trivial one (`[features] default = ["std"]; std
= []`) — keeps the per-crate forwarding pattern consistent.
Validated on RV32 by adding `cargo check -p lpfx-cpu --target
riscv32imac-unknown-none-elf --no-default-features` to the
validation matrix.

**Context.** `lpfx-cpu` is currently `#![no_std]` + `extern crate
alloc`. After M4c:

- RV32 path uses `lpvm-native` (`no_std` OK).
- wasm32 path uses `lpvm-wasm` browser runtime (needs `std` —
  uses `std::collections::HashMap`, `std::sync::Mutex`, etc.).
- Catchall (host) path uses `lpvm-wasm` wasmtime runtime (needs
  `std`).

So `lpfx-cpu` cannot stay strict `no_std` everywhere. Three
variants:

A. **Drop `no_std` entirely.** Always `std`. Loses RV32 / firmware
   compatibility — but lpfx-cpu has no in-tree firmware consumer
   today. Simplest; explicit about the fact that lpfx-cpu is
   host-class.

B. **Conditional: `#![cfg_attr(target_arch = "riscv32", no_std)]`.**
   `no_std` on RV32, `std` everywhere else. Preserves the option
   to use lpfx-cpu on RV32 firmware (would need its dep manifest
   to also gate `std` features by target). More moving parts;
   value depends on whether anyone actually plans to embed
   lpfx-cpu in firmware.

C. **Keep `#![no_std]` strict; have catchall code use only
   `core` + `alloc` types (rules out `lpvm-wasm`'s std
   primitives).** Not feasible — `lpvm-wasm` requires std.

**Suggested answer.** **A — drop `#![no_std]`.** No in-tree
firmware consumer of `lpfx-cpu`; the whole point of `lpfx` /
`lpfx-cpu` is the standalone authoring runtime, and the lpfx
roadmap places `lpfx-gpu` (wgpu, std-only) as a sibling. Keeping
the no_std attribute as theatre while three-quarters of the actual
deps require std would just confuse future readers. If embedded
use becomes a real ask later, option B is the additive escape
hatch; until then, `std` everywhere matches reality.

### Q5. Engine reuse vs. per-instance engine ✅ resolved

**Answer.** **A — single `LpsEngine` per `CpuFxEngine`,
constructed in `CpuFxEngine::new`, reused across all
`instantiate` and `create_texture` calls.** This is the design
goal: engines are 1-to-1-to-1 — one `CpuFxEngine` owns one
`LpsEngine`, which owns one `LpvmEngine`. The current
per-`instantiate` `CraneliftEngine::new` is recorded as a
pre-existing mistake corrected by this migration.

Implications:

- All textures and all compiled shaders for one `CpuFxEngine`
  share the underlying `LpvmMemory` pool.
- Dropping an `FxInstance` doesn't reclaim its texture bytes
  (bump allocator); only dropping the whole `CpuFxEngine`
  releases the pool. Bounded by M4b's 64 MiB pre-grown wasmtime
  memory on host (≈ 8M Rgba16Unorm pixels). Document this
  bounded pool in `CpuFxEngine::new`'s docstring.

Deferred (sub-question): a `CpuFxEngine::from_engine(LpsEngine<E>)`
constructor for the future `lp-engine`-consumes-`lpfx-cpu` case.
Add when the integration actually needs it; not in M4c scope.

**Context.** Today `CpuFxEngine::instantiate` constructs a fresh
`CraneliftEngine` per call. After M4c, the natural shape (and the
one the roadmap shows) is to hold one `LpsEngine<E>` on
`CpuFxEngine` and reuse it across all `instantiate` calls. This
matters because:

- `LpsEngine::alloc_texture` allocates from the engine's
  `LpvmEngine::memory()`, so all textures (and any future
  guest-visible buffers) sit in one shared pool.
- The engine's compile state (compiled modules, JIT caches, the
  wasmtime `Engine`) is reused across shaders, which is a real
  cost saving on host.
- `WasmLpvmEngine` constructs a wasmtime `Engine` + `Store` +
  `Memory`; per-instance construction would multiply that cost.

**Options.**

A. **Single engine per `CpuFxEngine`, constructed in
   `CpuFxEngine::new`, reused across `instantiate` and
   `create_texture`.** Mirrors `lp_engine::Graphics::new`.

B. **Engine per instance.** Simple but defeats the point of
   `LpsEngine` — every shader has its own memory pool, no
   sharing, no caching. Heavy on the host.

**Suggested answer.** **A.** No question really, but worth
recording. `CpuFxEngine::new` does
`LpsEngine::new(WasmLpvmEngine::new(WasmOptions::default())…)` (or
the target-equivalent); textures and instances both come out of
that engine.

**Sub-question.** What about texture lifetime? Today
`instantiate(module, output)` removes the `CpuTexture` from the
engine's pool and moves it onto the instance. With `LpsTextureBuf`
that still works (the buffer holds an `LpvmBuffer` whose host
pointer outlives the instance until the instance is dropped). The
shared memory backing is owned by the engine's `LpvmEngine`, not
freed by `LpvmBuffer::drop` — bump allocator. This is fine for
M4c; over-allocation is bounded by the user's number of
instantiate calls. (M4b's pre-grown 64 MiB budget gives ~8M pixels
of Rgba16Unorm before OOM — plenty for any reasonable lpfx
session.)

### Q6. Uniform writeback — per-render rebuild or per-input set? ✅ resolved

**Answer.** **Reshape the `FxInstance` trait to mirror
`LpsPxShader::render_frame`'s contract: take all uniforms on each
render call, drop `set_input` from the trait.**

Concrete shape (in `lpfx/lpfx/src/engine.rs`):

```rust
pub trait FxInstance {
    type Error: core::fmt::Display;

    fn render(&mut self, inputs: &FxRenderInputs<'_>) -> Result<(), Self::Error>;
}

pub struct FxRenderInputs<'a> {
    pub time: f32,
    pub inputs: &'a [(&'a str, FxValue)],
}
```

Sub-decisions:

- **Slice form** for `inputs` (no per-call allocation, no
  `BTreeMap`, no generic on the trait method, no dep on
  `lps-shared` from `lpfx`).
- **`time` is a typed field** on `FxRenderInputs` (not buried as a
  string-keyed entry in the slice). Frame clock is mandatory and
  type-distinct from optional user inputs.
- **`set_input` removed** from `FxInstance`. The cached
  per-instance uniform map disappears entirely.
- **Manifest defaults** move out of `instantiate` into a
  caller-side helper on `lpfx` (e.g.
  `lpfx::defaults_from_manifest(&FxManifest) -> Vec<(String, FxValue)>`).
  Caller threads its own `Vec` into `FxRenderInputs.inputs` per
  frame, overlaying any values it wants to drive.
- `outputSize` stays derived from the bound output texture (it's
  set up at `instantiate` time and doesn't change between renders).

`lpfx-cpu`'s `CpuFxInstance::render` builds the
`LpsValueF32::Struct` for `LpsPxShader::render_frame` from
`outputSize`, `time`, and the supplied inputs slice each call.
No state cached on the instance for inputs.

**Future work flagged (recorded under Notes):** GLSL `layout(binding
= N)` uniform "slots" — `lpfx-cpu` currently identifies uniforms by
name (`input_X`), ignoring the slot/binding-index concept entirely.
This shape works for now but will need rework when slot-based
uniform addressing comes online. Revisit when that lands.

**Context.** Today `set_input(name, FxValue)` calls
`instance.set_uniform(uniform_name, &lps_val)` directly on the
`LpvmInstance`, mutating its uniform region in place. `render(t)`
just runs the shader; uniforms persist between frames. Cheap.

`LpsPxShader` exposes only `render_frame(&uniforms, &mut tex)`,
which takes the **full** uniforms struct and applies all members
each call (private `apply_uniforms` walks `uniforms_type.members`).
There is no public `set_one_uniform` API on `LpsPxShader`.

**Options.**

A. **Cache `FxValue`s on the instance; rebuild
   `LpsValueF32::Struct` each render.**
   - `set_input(name, val)` writes into a
     `BTreeMap<String, LpsValueF32>` on the instance.
   - `render(time)` walks the cache, builds a struct
     `{ outputSize, time, input_X, … }`, calls `render_frame`.
   - Cost: one map walk + struct build per frame. Trivial vs. the
     pixel loop.

B. **Add a public per-uniform setter to `LpsPxShader`.** Expose
   `set_uniform(path, value)` so `set_input` keeps the in-place
   mutation it has today. Smaller delta to lpfx-cpu's code shape;
   widens the `lp-shader` API surface for one consumer.

C. **Hybrid.** Cache values on the instance AND apply them to the
   shader on `set_input`; pass an empty struct on `render_frame`.
   Doesn't work: `apply_uniforms` rejects missing fields when
   `uniforms_type` has members.

**Suggested answer.** **A — rebuild per render.** Matches the
`render_frame(&uniforms, …)` API as designed. Per-frame map walk
is negligible vs. the pixel loop. Avoids growing `lp-shader`'s
public surface for an `lpfx`-only need. The cached
`BTreeMap<String, LpsValueF32>` also gives `lpfx-cpu` a clean
seed-from-defaults step at instantiate time.

If profiling later shows the per-frame rebuild matters (it
won't), option B can be added then as an opt-in fast path.

### Q7. Plan name and location ✅ resolved

**Answer.** Active directory:
`docs/plans/2026-04-19-m4c-lpfx-cpu-migration/`. On completion,
move the whole directory to `docs/plans-old/`. (User chose to
keep using `plans-old/` rather than introducing `plans-done/` for
now.)

### Q8. Implementation timing relative to M4b ✅ resolved

**Answer.** M4b is treated as done for the purposes of this plan
(per user — it is either merged or about to be). M4c phases are
written assuming M4b's end-state and dispatched against a tree
where M4b has landed. No parallel-execution concerns remain.

**Context.** The user explicitly wants to plan M4c while M4b is
mid-implementation. The two plans live in the same working tree
right now (M4b's diff is uncommitted; M4c will be its own commit).
The roadmap allows parallel execution but recommends
sequential a → b → c.

**Options.**

A. **Plan now, dispatch sub-agents only after M4b commits.** Phase
   files are written assuming M4b's end-state (no `cranelift`
   feature, target-arch dispatch, `lp_engine::Graphics`). If M4b
   shifts during execution, the M4c phases get a small re-edit
   before dispatch. Lowest risk of merge conflicts and
   re-implementation churn.

B. **Plan now, dispatch in parallel with M4b.** Two streams of
   target-arch refactors against shared deps (`lpvm-wasm`,
   `lps-shared`). High risk of merge conflicts in `Cargo.toml`s
   and the `lpvm-wasm` test layout. Nothing in M4c blocks on M4b
   *file-wise* (M4c only touches `lpfx/`), but the conceptual
   model is dependent.

C. **Defer planning until M4b is done.** Wastes the planning
   window; M4c is clear enough to plan now.

**Suggested answer.** **A.** Write the plan now, hold sub-agent
dispatch until M4b commits. Phase files reference the M4b
end-state explicitly; cleanup phase verifies the workspace
post-merge.

# Notes (raw / unresolved)

- **`lp-engine` will soon consume `lpfx`** (per user, during Q4).
  This makes `lpfx-cpu`'s RV32 / `no_std` story binding rather
  than aspirational, and means `lpfx`'s public API can't churn
  carelessly — each break propagates into `lp-engine`. Affects:
  - Q4 → resolved as "thread `std` through, mirror `lp-engine`'s
    pattern" (vs. the original suggestion of dropping `no_std`).
  - The texture API choices in Q3 are still fine — `lp-engine`
    already speaks `LpsTextureBuf` natively, so converging
    `lpfx-cpu`'s `output()` on `&LpsTextureBuf` is a forward-fit.
  - The cleanup phase should add `cargo check -p lpfx-cpu
    --target riscv32imac-unknown-none-elf --no-default-features`
    to the validation matrix to prove the firmware path keeps
    compiling.

- **GLSL uniform "slots" / `layout(binding = N)` not wired up.**
  Both `lpfx-cpu` today and the post-M4c shape identify uniforms
  by name (`input_X`). GLSL's `layout(binding = N)` slot/binding
  index concept is being ignored across the workspace
  (`noise.fx` uses `layout(binding = 0)` on every uniform —
  syntactically present, semantically unused). This is fine for
  M4c but is an open future-work item — when slot-based uniform
  addressing is wired up properly, the `FxRenderInputs` shape
  (currently `&[(&str, FxValue)]`) will likely need to switch to
  slot-indexed addressing, and `LpsPxShader::set_uniform` /
  `apply_uniforms` will follow. Revisit when that work starts;
  the slice-of-name-keyed-pairs shape was chosen with explicit
  awareness that it'll be replaced.
