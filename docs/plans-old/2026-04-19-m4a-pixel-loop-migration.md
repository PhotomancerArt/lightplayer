# M4a — Pixel-Loop Migration (plan-small)

Roadmap milestone:
[`docs/roadmaps/2026-04-16-lp-shader-textures/m4a-pixel-loop-migration.md`](../roadmaps/2026-04-16-lp-shader-textures/m4a-pixel-loop-migration.md)

# Design

## Scope of work

Move the per-pixel render loop and the Q32 → unorm16 conversion out of
`lp-engine`'s graphics backends and into `lp-shader::LpsPxShader::render_frame`
(which delegates to the synthesised `__render_texture_<format>` LPIR
function from M2). Both live host paths get the treatment **without
changing which LPVM backend they use**:

- `lp-core/lp-engine/src/gfx/cranelift.rs` — desktop / `fw-emu`,
  `lpvm-cranelift`. Hand-rolled `render_direct_call` deleted.
- `lp-core/lp-engine/src/gfx/native_jit.rs` — ESP32 firmware,
  `lpvm-native`. Hand-rolled `render_native_jit_direct` deleted.

Three additional pieces of work fold in because they're prerequisites or
direct consequences:

- **Phase 0 bug fix:** `lpvm_native::NativeJitEngine::compile` silently
  drops `CompilerConfig` (only `float_mode` and `alloc_trace` are
  forwarded). Must be fixed before M4a routes engine compilation
  through `LpvmEngine::compile`.
- **Shader signature migration:** all engine shaders converted from
  legacy `vec4 render(vec2 fragCoord, vec2 outputSize, float time)` to
  `vec4 render(vec2 pos)` with `outputSize` / `time` as uniforms.
- **Texture-storage refactor:** the engine's per-shader render target
  becomes an `LpsTextureBuf` (allocated once from the shader's
  `LpsEngine`, reused across frames). `lp_shared::Texture` is marked
  `#[deprecated]`. Consumer-facing API on `RenderContext` returns
  `&dyn lps_shared::TextureBuffer`. No per-frame copy, no double
  allocation on hardware.

The host backend swap (Cranelift → Wasmtime) is M4b. `lpfx-cpu` is M4c.

## File structure

```
lp-shader/
├── lp-shader/src/
│   ├── engine.rs                          # UPDATE: compile_px gains &CompilerConfig
│   ├── px_shader.rs                       # (no change expected)
│   └── texture_buf.rs                     # (no change expected)
├── lpvm-native/src/
│   └── rt_jit/
│       ├── engine.rs                      # UPDATE: forward NativeCompileOptions
│       └── compiler.rs                    # UPDATE: take &NativeCompileOptions
│
lp-core/
├── lp-shared/src/util/
│   └── texture.rs                         # UPDATE: #[deprecated] + impl TextureBuffer
├── lp-engine/src/
│   ├── gfx/
│   │   ├── lp_shader.rs                   # UPDATE: LpShader trait
│   │   ├── cranelift.rs                   # REWRITE: hold LpsEngine<CraneliftEngine>
│   │   └── native_jit.rs                  # REWRITE: hold LpsEngine<NativeJitEngine>
│   ├── nodes/shader/
│   │   └── runtime.rs                     # UPDATE: own buffer, set uniforms
│   ├── nodes/texture/
│   │   └── runtime.rs                     # UPDATE: spec-only (no buffer ownership)
│   ├── nodes/fixture/
│   │   └── runtime.rs                     # UPDATE: consume &dyn TextureBuffer
│   ├── runtime/
│   │   └── contexts.rs                    # UPDATE: get_texture -> &dyn TextureBuffer
│   ├── project/
│   │   └── runtime.rs                     # UPDATE: route textures via shader
│   └── tests/
│       └── scene_update.rs                # UPDATE: shader source uses new sig
│
examples/                                   # UPDATE: convert engine shaders
├── basic/src/rainbow.shader/main.glsl
├── basic2/src/rainbow.shader/main.glsl
├── fast/src/simple.shader/main.glsl
└── mem-profile/src/rainbow.shader/main.glsl
# noise.fx/main.glsl deferred to M4c
lp-app/web-demo/www/
└── rainbow-default.glsl                    # UPDATE: new render signature
lp-core/
├── lp-shared/src/project/builder.rs       # UPDATE: default-project shader string
└── lp-server/src/template.rs              # UPDATE: server template
lp-cli/src/commands/create/
└── project.rs                              # UPDATE: lp-cli create templates
```

## Conceptual architecture

```
┌─────────────────────── lp-engine (host) ────────────────────────┐
│                                                                  │
│  ShaderConfig + GLSL ──► CraneliftGraphics::compile_shader      │
│                                  │                               │
│                                  ▼                               │
│   one LpsEngine<CraneliftEngine> per Graphics, reused           │
│                                  │                               │
│                                  │ compile_px(glsl, fmt, &cfg)  │
│                                  ▼                               │
│                          LpsPxShader  ◄── synthesised           │
│                                  │       __render_texture_rgba16 │
│                                  ▼                               │
│   per-shader LpsTextureBuf (allocated once, reused per frame)   │
│   stored in ShaderRuntime, exposed via RenderContext as          │
│   &dyn TextureBuffer                                             │
│                                                                  │
│  per frame:                                                      │
│   ShaderRuntime::render(ctx) ──► shader.render(buf, uniforms)    │
│                                       │                          │
│                                       ▼                          │
│              LpsPxShader::render_frame(uniforms, &mut buf)       │
│                  apply uniforms (outputSize, time, …)            │
│                  call __render_texture_rgba16(buf, w, h)         │
│                                                                  │
│  FixtureRuntime ─► ctx.get_texture(handle) ─► &dyn TextureBuffer│
│                       buf.data() / buf.format() / buf.width()    │
└──────────────────────────────────────────────────────────────────┘

ESP32 firmware path: identical, with NativeJitEngine in place of
CraneliftEngine, reusing the existing Arc<BuiltinTable>.
```

Key invariants:

- **One `LpsEngine` per `Graphics`**, reused across `compile_shader`
  calls. `BuiltinTable` (lpvm-native) constructed once and `Arc`'d.
  All texture buffers are allocated from this single engine's memory
  pool — important for `lpvm-native` JIT, which requires guest
  pointers to live in its own pool.
- **One `LpsTextureBuf` per shader-target**, allocated from that
  engine once, reused across frames. Output and consumer touch the
  same shared memory — no copy.
- **`compile_px(glsl, format, &CompilerConfig)`** — config per call.
- **Output format** = `Rgba16Unorm`. Same byte layout as today's
  `TextureFormat::Rgba16`.
- **Pixel coordinate semantics:** `pos` is Q32 of pixel **center**
  (i.e. x=0 → 0.5). Confirmed acceptable.
- **Buffer ownership shifts** from `TextureRuntime` to
  `ShaderRuntime`. `TextureRuntime` retains its dimensions/format
  spec but no longer owns a buffer. (Today there are no
  shader-independent texture sources, so no consumer is left
  stranded.)

# Phases

## Phase 0 — Fix `lpvm-native` `LpvmEngine::compile` config drop  [sub-agent: yes]

### Scope of phase

Fix the bug where `lpvm_native::NativeJitEngine::compile` silently drops
`CompilerConfig` because `compile_module_jit` only forwards
`float_mode` and `alloc_trace`, then constructs a fresh
`NativeCompileOptions { ..Default::default() }` internally.

**Out of scope:** any other engine plumbing changes; do not touch
`compile_module` or its callers other than the strict signature
forwarding.

### Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

### Sub-agent Reminders

- Do **not** commit. The plan commits at the end as a single unit.
- Do **not** expand scope. Stay strictly within "Scope of phase".
- Do **not** suppress warnings or `#[allow(...)]` problems away — fix them.
- Do **not** disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report rather than improvising.
- Report back: what changed, what was validated, any deviations.

### Implementation Details

Files:

- `lp-shader/lpvm-native/src/rt_jit/compiler.rs`
- `lp-shader/lpvm-native/src/rt_jit/engine.rs`
- `lp-shader/lpvm-native/src/native_options.rs` (read-only reference)

Today (`compiler.rs:35-49`):

```rust
pub fn compile_module_jit(
    ir: &LpirModule,
    sig: &LpsModuleSig,
    builtin_table: &BuiltinTable,
    float_mode: lpir::FloatMode,
    _alloc_trace: bool,
    isa: IsaTarget,
) -> Result<(JitBuffer, BTreeMap<String, usize>, ModuleDebugInfo), NativeError> {
    let options = NativeCompileOptions {
        float_mode,
        debug_info: false,
        emu_trace_instructions: false,
        alloc_trace: false,
        ..Default::default()
    };
    // ...
    let compiled = compile_module(ir, sig, float_mode, options, isa)?;
    // ...
}
```

Today (`engine.rs:45-64`):

```rust
fn compile(&self, ir: &LpirModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
    let (buffer, entry_offsets, _debug_info) = compile_module_jit(
        ir, meta, &self.builtin_table,
        self.options.float_mode,
        self.options.alloc_trace,
        IsaTarget::Rv32imac,
    )?;
    // ...
    Ok(NativeJitModule { inner: Arc::new(NativeJitModuleInner {
        // ... uses self.options.clone() here
        options: self.options.clone(),
        // ...
    })})
}
```

Required change:

1. Change `compile_module_jit`'s parameter list to take `&NativeCompileOptions`
   instead of `(float_mode, alloc_trace)`. Remove the internal
   construction of a fresh `NativeCompileOptions`. Pass through
   `options.float_mode` to `compile_module`.

   New signature:

   ```rust
   pub fn compile_module_jit(
       ir: &LpirModule,
       sig: &LpsModuleSig,
       builtin_table: &BuiltinTable,
       options: &NativeCompileOptions,
       isa: IsaTarget,
   ) -> Result<(JitBuffer, BTreeMap<String, usize>, ModuleDebugInfo), NativeError>
   ```

2. Update `engine.rs::compile` to call `compile_module_jit(ir, meta,
   &self.builtin_table, &self.options, IsaTarget::Rv32imac)`.

3. Audit any other callers of `compile_module_jit` and update them
   similarly.

4. **Add a unit test** in `lp-shader/lpvm-native/src/rt_jit/` or the
   nearest sensible test file: assert that compiling a tiny LPIR
   module via `NativeJitEngine::compile` with non-default
   `Q32Options` (e.g. `MulMode::Wrapping`) actually selects the
   wrapping helper, while default selects the saturating helper. The
   simplest evidence is symbol presence: lower a function that
   contains an `Fmul`, then check the linked symbols include
   `lpfx_q32_fmul_wrapping` (or whatever the wrapping helper is
   named) for the wrapping case and `lpfx_q32_fmul_saturating` for
   the default. If symbol names aren't readily inspectable, lower an
   IR with both modes and assert the produced byte buffers differ.

### Validate

```
cargo check -p lpvm-native
cargo test  -p lpvm-native
```

Both must pass with the new test. No new clippy warnings or unused-import
warnings.

## Phase 1 — `compile_px` gains `&CompilerConfig`  [sub-agent: yes]

### Scope of phase

Extend `LpsEngine::compile_px` to accept `&lpir::CompilerConfig` per call
and forward it into the underlying backend's compile path. Update
existing callers (lp-shader's tests).

**Out of scope:** any changes outside `lp-shader` and its test set; no
plumbing into `lp-engine` yet (Phase 3 does that).

### Code Organization Reminders

(See Phase 0.)

### Sub-agent Reminders

(See Phase 0.)

### Implementation Details

Files:

- `lp-shader/lp-shader/src/engine.rs` — `LpsEngine::compile_px`
- `lp-shader/lp-shader/src/tests.rs` — all callers
- (read-only) `lp-shader/lpvm-native/src/native_options.rs` —
  `NativeCompileOptions::config: lpir::CompilerConfig`
- (read-only) `lp-shader/lpvm-cranelift/src/compile_options.rs` —
  `CompileOptions::config: lpir::CompilerConfig`

Today (`engine.rs:35-69`):

```rust
pub fn compile_px(
    &self,
    glsl: &str,
    output_format: TextureStorageFormat,
) -> Result<LpsPxShader, LpsError>
where E::Module: 'static
```

`LpsEngine` calls `self.engine.compile(&ir, &meta)` via the trait
`LpvmEngine`. `LpvmEngine::compile` does **not** take per-call options;
its options are baked into the engine at construction. To honour a
per-call `CompilerConfig`, the cleanest path is:

**Option A (recommended):** Add a sibling trait method
`LpvmEngine::compile_with_config(&self, ir, meta, &CompilerConfig)`
with a default impl that **panics or errors** ("must be overridden if
`compile_px` with explicit config is used"), and override it on both
`CraneliftEngine` and `NativeJitEngine` to actually use the supplied
config. Wire `compile_px` to call `compile_with_config`.

**Option B:** Re-construct the LPVM engine inside `compile_px` from a
factory — too invasive, drops Phase 0's per-Graphics reuse goal.

**Option C:** Have `LpvmEngine` expose a mutable "set config" method.
Mutates engine state — race-prone if engines are ever shared.

Pick **A**. New trait shape (in `lp-shader/lpvm/src/lib.rs` —
locate the `LpvmEngine` trait def first; if it lives elsewhere,
update there):

```rust
pub trait LpvmEngine {
    type Module: LpvmModule;
    type Error: core::fmt::Display;

    fn compile(&self, ir: &LpirModule, meta: &LpsModuleSig)
        -> Result<Self::Module, Self::Error>;

    /// Compile with an explicit per-call `CompilerConfig`. Default impl
    /// returns an error indicating the engine doesn't yet support it;
    /// engines that thread per-call config (Cranelift, NativeJit) must
    /// override. WASM may follow in M4b.
    fn compile_with_config(
        &self,
        ir: &LpirModule,
        meta: &LpsModuleSig,
        _config: &lpir::CompilerConfig,
    ) -> Result<Self::Module, Self::Error> {
        // Default: ignore config and fall back to engine-construction-time settings.
        self.compile(ir, meta)
    }

    fn memory(&self) -> &dyn LpvmMemory;
}
```

Override on `CraneliftEngine` (`lpvm-cranelift/src/lpvm_engine.rs`):

```rust
fn compile_with_config(
    &self,
    ir: &LpirModule,
    meta: &LpsModuleSig,
    config: &lpir::CompilerConfig,
) -> Result<Self::Module, Self::Error> {
    let mut opts = self.options.clone();
    opts.config = config.clone();
    opts.q32_options = config.q32; // mirror until q32_options field is removed
    CraneliftModule::compile(ir, meta, opts)
}
```

Override on `NativeJitEngine` (`lpvm-native/src/rt_jit/engine.rs`,
**after** Phase 0 is in):

```rust
fn compile_with_config(
    &self,
    ir: &LpirModule,
    meta: &LpsModuleSig,
    config: &lpir::CompilerConfig,
) -> Result<Self::Module, Self::Error> {
    let mut opts = self.options.clone();
    opts.config = config.clone();
    let (buffer, entry_offsets, _debug_info) =
        compile_module_jit(ir, meta, &self.builtin_table, &opts, IsaTarget::Rv32imac)?;
    Ok(NativeJitModule { inner: Arc::new(NativeJitModuleInner {
        ir: ir.clone(), meta: meta.clone(), buffer, entry_offsets,
        options: opts, isa: IsaTarget::Rv32imac,
    })})
}
```

`compile_px` change:

```rust
pub fn compile_px(
    &self,
    glsl: &str,
    output_format: TextureStorageFormat,
    config: &lpir::CompilerConfig,
) -> Result<LpsPxShader, LpsError>
where E::Module: 'static,
{
    // ... unchanged through synth ...
    let module = self
        .engine
        .compile_with_config(&ir, &meta, config)
        .map_err(|e| LpsError::Compile(format!("{e}")))?;
    LpsPxShader::new(module, meta, output_format, render_fn_index, render_texture_fn_name)
}
```

**Caller update:** `lp-shader/lp-shader/src/tests.rs` has many
`engine.compile_px(glsl, fmt)` call sites. Add `&CompilerConfig::default()`
as the new third argument to each. (Search for `compile_px(` in
`lp-shader/lp-shader/src/tests.rs`.)

### Validate

```
cargo check -p lp-shader -p lpvm-cranelift -p lpvm-native -p lpvm
cargo test  -p lp-shader
```

## Phase 2 — Texture API reshape  [sub-agent: yes]

### Scope of phase

Three deeply-interrelated refactors that must land together:

1. Add `impl lps_shared::TextureBuffer for lp_shared::Texture`. Mark
   `lp_shared::Texture` `#[deprecated]`.
2. Change `RenderContext::get_texture` to return
   `&dyn lps_shared::TextureBuffer` instead of `&lp_shared::Texture`.
   Drop `RenderContext::get_texture_mut` entirely (consumers only
   sample).
3. Move buffer ownership from `TextureRuntime` to `ShaderRuntime`.
   `TextureRuntime` keeps its config (dimensions/format) but no
   longer owns a `Texture`. `ShaderRuntime` allocates its buffer at
   `init` time using its target texture's dimensions, and exposes it
   via the `RenderContext` plumbing.

**Important:** this phase keeps `Texture` as the concrete buffer type
inside `ShaderRuntime`. The swap to `LpsTextureBuf` happens in Phase 3.
This isolates the API/ownership refactor from the gfx migration so
each is independently reviewable.

**Out of scope:**
- gfx backend migration (Phase 3, 4).
- shader signature conversion (Phase 3).
- removing `Texture` (it stays; only deprecated).

### Code Organization Reminders

(See Phase 0.)

### Sub-agent Reminders

(See Phase 0.)

### Implementation Details

Files (read carefully before editing):

- `lp-core/lp-shared/src/util/texture.rs`
- `lp-shader/lps-shared/src/texture_buffer.rs` (read-only reference)
- `lp-shader/lps-shared/src/texture_format.rs` (read-only reference;
  format mapping below)
- `lp-core/lp-engine/src/runtime/contexts.rs`
- `lp-core/lp-engine/src/project/runtime.rs` (`RenderContextImpl`,
  `ensure_texture_rendered`)
- `lp-core/lp-engine/src/nodes/texture/runtime.rs`
- `lp-core/lp-engine/src/nodes/shader/runtime.rs`
- `lp-core/lp-engine/src/nodes/fixture/runtime.rs`
- `lp-core/lp-engine/src/gfx/lp_shader.rs` (the trait)
- `lp-core/lp-engine/src/gfx/cranelift.rs` (legacy render path,
  needs to take `&mut Texture` from ShaderRuntime not from context)
- `lp-core/lp-engine/src/gfx/native_jit.rs` (same)
- `lp-core/lp-engine/tests/scene_update.rs`
- `lp-core/lp-engine/src/nodes/output/runtime.rs` (uses get_texture? check)

#### Format mapping

`lp_shared::TextureFormat::Rgba16` ↔ `lps_shared::TextureStorageFormat::Rgba16Unorm`.
`Rgb8`, `Rgba8`, `R8`, `Rgb16` are not yet in `TextureStorageFormat`;
for the `TextureBuffer` impl, return `Rgba16Unorm` when the format is
`Rgba16`, and otherwise either:
  - panic with a clear message (these formats aren't used in the
    shader-output path today), or
  - extend `TextureStorageFormat` with parallel variants.

Recommended: panic with `unimplemented!("TextureFormat {:?} has no
TextureStorageFormat mapping yet", self.format())`. The shader path
only ever uses `Rgba16`. If the panic ever fires we'll add a variant
on demand.

#### `TextureBuffer` impl

In `lp-core/lp-shared/src/util/texture.rs`:

```rust
impl lps_shared::TextureBuffer for Texture {
    fn width(&self) -> u32 { self.width() }
    fn height(&self) -> u32 { self.height() }
    fn format(&self) -> lps_shared::TextureStorageFormat {
        match self.format() {
            crate::util::formats::TextureFormat::Rgba16 =>
                lps_shared::TextureStorageFormat::Rgba16Unorm,
            other => unimplemented!("..."),
        }
    }
    fn data(&self) -> &[u8] { self.data() }
    fn data_mut(&mut self) -> &mut [u8] { self.data_mut() }
}
```

Add the `lps-shared` dep to `lp-core/lp-shared/Cargo.toml` if it
isn't already there.

#### Deprecation

```rust
#[deprecated(
    since = "0.x.0",
    note = "use lps_shared::TextureBuffer / LpsTextureBuf via lp-shader; \
            will be removed once all texture sources migrate"
)]
pub struct Texture { /* unchanged */ }
```

Then `#[allow(deprecated)]` on the impl blocks and any internal
self-references to suppress warnings within `lp-shared`. External
callers will surface as warnings — that's the point.

#### `RenderContext`

In `lp-core/lp-engine/src/runtime/contexts.rs`:

- Replace import `use lp_shared::Texture;` with appropriate trait import.
- Change:
  ```rust
  fn get_texture(&mut self, handle: TextureHandle) -> Result<&dyn lps_shared::TextureBuffer, Error>;
  ```
- **Remove** `get_texture_mut`.

#### `RenderContextImpl` (project/runtime.rs)

`get_texture` no longer routes through `TextureRuntime` (it doesn't
own the buffer anymore). Instead, route to the `ShaderRuntime` whose
target is this texture handle. Find that shader by scanning
`self.nodes` for any `ShaderRuntime` with matching texture_handle.
Helper:

```rust
fn find_shader_for_texture(
    nodes: &mut BTreeMap<NodeHandle, NodeEntry>,
    texture_handle: TextureHandle,
) -> Option<&mut ShaderRuntime> { /* scan and downcast */ }
```

If no shader writes to this texture, return an error (or, for
backward compat with no-shader textures, fall back to TextureRuntime
holding a default-zero `Texture` — but verify no current consumer
needs that path; if none, just error).

`ensure_texture_rendered` continues to drive lazy rendering on the
upstream shader, then `get_texture` returns the shader's buffer as
`&dyn TextureBuffer`.

#### `TextureRuntime`

- Remove `texture: Option<Texture>` field.
- Remove `texture()`, `texture_mut()`, `ensure_texture()` methods that
  return `Texture` references.
- Keep dimensions/format spec accessors (`get_config`, `get_state`,
  etc.) — fixtures/state extraction may use them.
- `state.texture_data` — currently set from `tex.data().to_vec()`.
  This was used for state extraction. Either drop these fields or
  populate them from the upstream shader's buffer. **Decision for this
  phase:** drop the population (set to empty vec) and add a TODO:
  "TODO(M4a): texture_data state should come from upstream shader's buffer".

#### `ShaderRuntime`

- Add `output_buffer: Option<Texture>` field, allocated in `init()`
  using the target texture's dimensions (resolve via texture handle →
  TextureRuntime config).
- Reallocate on `update_config` if texture_spec changed.
- New helper `output_buffer_mut(&mut self) -> Option<&mut Texture>`
  and `output_buffer(&self) -> Option<&Texture>` (or, better,
  `&dyn TextureBuffer`).
- `ShaderRuntime::render` (the `impl NodeRuntime`):
  - Stop calling `ctx.get_texture_mut(handle)`. Instead, use the
    locally-owned `output_buffer`.
  - Pass `output_buffer` into `shader.render(buffer, time)`.

#### `LpShader` trait (gfx/lp_shader.rs)

Change:

```rust
pub trait LpShader: Send + Sync {
    fn render(&mut self, texture: &mut dyn lps_shared::TextureBuffer, time: f32) -> Result<(), Error>;
    fn has_render(&self) -> bool;
}
```

Update both `cranelift.rs` and `native_jit.rs` legacy render paths to
take `&mut dyn TextureBuffer` and use `texture.data_mut()` to write
pixels (replacing `set_pixel_u16` calls — write the 8 bytes per pixel
directly into the slice).

This means the legacy 5-arg shader path keeps working: it just writes
into a different consumer-facing API. Visual output identical.

#### `FixtureRuntime`

- Update calls from `ctx.get_texture(handle)` to use the new return
  type. Sites read `.data()`, `.format()`, `.width()`, `.height()` —
  all available on `TextureBuffer`. Format check needs to compare
  against `TextureStorageFormat::Rgba16Unorm` (or convert; pick
  whichever is less invasive).

#### Other consumers

Search workspace for `ctx.get_texture` and `ctx.get_texture_mut` and
update each. Also check `nodes/output/runtime.rs`.

### Validate

```
cargo check --workspace --no-default-features
cargo check --workspace
cargo test  -p lp-engine
cargo test  -p lp-shared
```

Existing engine integration tests must still pass with **legacy
5-arg shaders** (Phase 3 is what converts them).

## Phase 3 — Migrate `gfx/cranelift.rs` + convert all shaders  [sub-agent: yes]

### Scope of phase

Atomically:

1. Replace `CraneliftGraphics::compile_shader` to use
   `LpsEngine<CraneliftEngine>::compile_px` and store the resulting
   `LpsPxShader`.
2. Rewrite `CraneliftShader::render` to call
   `LpsPxShader::render_frame(uniforms, buf)` after building the
   uniforms struct from `time` and `outputSize`.
3. Swap `ShaderRuntime::output_buffer` from `Texture` to
   `LpsTextureBuf` (allocated from the Graphics' shared `LpsEngine`).
4. Convert all engine-target GLSL shader sources and embedded shader
   strings from `vec4 render(vec2 fragCoord, vec2 outputSize, float time)`
   to `vec4 render(vec2 pos)` with explicit `outputSize` / `time`
   uniforms.
5. Delete `render_direct_call` from `cranelift.rs`.

These changes must land together: changing shader signature without
the gfx migration breaks legacy parsing; doing the gfx migration
without shader conversion makes `compile_px`'s validator reject the
shaders. Atomic phase.

**Out of scope:**
- `gfx/native_jit.rs` (Phase 4 — same shape, sequenced separately).
- `lpfx-cpu` and `noise.fx` (M4c).
- WASM emit cleanup (M4b).

### Code Organization Reminders

(See Phase 0.)

### Sub-agent Reminders

(See Phase 0.)

### Implementation Details

#### Shader conversions

Convert each of these from legacy 5-arg to new 1-arg + uniforms:

- `examples/basic/src/rainbow.shader/main.glsl`
- `examples/basic2/src/rainbow.shader/main.glsl`
- `examples/fast/src/simple.shader/main.glsl`
- `examples/mem-profile/src/rainbow.shader/main.glsl`
- `lp-app/web-demo/www/rainbow-default.glsl`
- `lp-core/lp-shared/src/project/builder.rs:117` (default shader string)
- `lp-core/lp-server/src/template.rs:58` (server template)
- `lp-cli/src/commands/create/project.rs:198, 482` (lp-cli create templates)
- `lp-core/lp-engine/tests/scene_update.rs:133` (test shader string)

(Skip `examples/noise.fx/main.glsl` — that's lpfx, M4c.)

Conversion pattern:

```glsl
// Before:
vec4 render(vec2 fragCoord, vec2 outputSize, float time) {
    // body uses fragCoord, outputSize, time
}

// After:
uniform vec2 outputSize;
uniform float time;

vec4 render(vec2 pos) {
    // identical body, with `fragCoord` rewritten to `pos`
}
```

Mechanical rename: `fragCoord` → `pos`. Keep the rest of each
shader's body untouched.

#### `gfx/lp_shader.rs::ShaderCompileOptions`

Already has `q32_options` and `max_errors`. Add a method (or just
inline) to convert into `lpir::CompilerConfig`:

```rust
impl ShaderCompileOptions {
    pub fn to_compiler_config(&self) -> lpir::CompilerConfig {
        lpir::CompilerConfig {
            q32: self.q32_options,
            ..Default::default()
        }
    }
}
```

Keep `max_errors` field with its existing TODO; do not wire.

#### `gfx/cranelift.rs` rewrite

```rust
use lp_shader::LpsEngine;
use lpvm_cranelift::{CompileOptions, CraneliftEngine};

pub struct CraneliftGraphics {
    engine: lp_shader::LpsEngine<lpvm_cranelift::CraneliftEngine>,
}

impl CraneliftGraphics {
    pub fn new() -> Self {
        let backend = CraneliftEngine::new(CompileOptions::default());
        Self { engine: LpsEngine::new(backend) }
    }
}

impl LpGraphics for CraneliftGraphics {
    fn compile_shader(
        &self,
        source: &str,
        options: &ShaderCompileOptions,
    ) -> Result<Box<dyn LpShader>, Error> {
        let cfg = options.to_compiler_config();
        let px = self.engine.compile_px(
            source,
            lps_shared::TextureStorageFormat::Rgba16Unorm,
            &cfg,
        )?;
        Ok(Box::new(CraneliftShader { px }))
    }
    fn backend_name(&self) -> &'static str { "cranelift" }
}

struct CraneliftShader { px: lp_shader::LpsPxShader }

impl LpShader for CraneliftShader {
    fn render(&mut self, buf: &mut dyn lps_shared::TextureBuffer, time: f32)
        -> Result<(), Error>
    {
        // The buf passed in is the engine's per-shader output buffer.
        // It must be the LpsTextureBuf that was allocated from this engine.
        // (See ShaderRuntime change below.) Downcast or accept &mut LpsTextureBuf
        // directly via a different LpShader trait shape — TBD by sub-agent.
    }
    fn has_render(&self) -> bool { true }
}
```

**Trait shape decision:** `LpShader::render` currently takes `&mut dyn TextureBuffer`
(after Phase 2). But `LpsPxShader::render_frame` requires
`&mut LpsTextureBuf`. The simplest reconciliation:

- Change `LpShader::render` to take `&mut LpsTextureBuf` directly
  (i.e. `LpShader` is no longer texture-buffer-impl-agnostic). Acceptable
  because by Phase 3 the only buffer type for shader output IS
  `LpsTextureBuf`. Remove the `&mut dyn TextureBuffer` from
  `LpShader::render` (revert to concrete type).
- `RenderContext::get_texture` continues to return `&dyn TextureBuffer`
  for consumers — that's a different code path.

Update Phase 2's `LpShader::render` signature accordingly when this
phase lands. (Phase 2 sets up the consumer-facing trait API; Phase 3
locks the producer to `LpsTextureBuf`.)

#### `ShaderRuntime` swap

Swap `output_buffer: Option<Texture>` → `output_buffer: Option<LpsTextureBuf>`.
Allocation now goes through the Graphics:

The engine's `LpGraphics` trait needs a way to allocate the buffer
from its own engine's memory. Add to `LpGraphics`:

```rust
pub trait LpGraphics: Send + Sync {
    fn compile_shader(...) -> Result<Box<dyn LpShader>, Error>;
    fn backend_name(&self) -> &'static str;

    /// Allocate a shader output buffer in the graphics engine's memory.
    fn alloc_output_buffer(&self, width: u32, height: u32)
        -> Result<lp_shader::LpsTextureBuf, Error>;
}
```

Implement on `CraneliftGraphics` by calling
`self.engine.alloc_texture(w, h, Rgba16Unorm)`.

`ShaderRuntime::init` now calls `graphics.alloc_output_buffer(w, h)`.
The `Graphics` reference flows through `NodeInitContext`, or pass it
explicitly when constructing `ShaderRuntime`. (Inspect existing code
to find the cleanest plumbing — if `Graphics` is held by
`ProjectRuntime` or similar, expose it via `NodeInitContext`.)

#### Uniforms

Build the uniforms struct from `time` and `outputSize`:

```rust
let uniforms = LpsValueF32::Struct {
    name: None,
    fields: alloc::vec![
        (String::from("outputSize"),
            LpsValueF32::Vec2([buf.width() as f32, buf.height() as f32])),
        (String::from("time"), LpsValueF32::F32(time)),
    ],
};
self.px.render_frame(&uniforms, buf)?;
```

Order of `fields` should follow declaration order in the GLSL (check
`apply_uniforms` impl in `lp-shader/lp-shader/src/px_shader.rs` —
matches by name, so order shouldn't matter, but verify).

#### `RenderContextImpl::get_texture` update

Already routes via `find_shader_for_texture` (Phase 2). The
shader's buffer is now `LpsTextureBuf` which `impl TextureBuffer`. No
change beyond the type swap.

### Validate

```
cargo check --workspace
cargo test  -p lp-engine
cargo test  -p lp-shader
cargo run --example basic --quiet -- --headless --frames 5  # or equivalent quick smoke
```

(Adjust the `cargo run` invocation to whatever the basic example
expects; goal is to render a few frames without panic and inspect
that output isn't all-zero.)

If an `fw-emu` smoke is straightforward, run that too.

## Phase 4 — Migrate `gfx/native_jit.rs` to `LpsEngine<NativeJitEngine>`  [sub-agent: yes]

### Scope of phase

Mirror Phase 3 for `lp-core/lp-engine/src/gfx/native_jit.rs`.

The `BuiltinTable` (`Arc<BuiltinTable>`) construction stays in
`NativeJitGraphics::new`; pass it into `NativeJitEngine::new` once and
hand the resulting `NativeJitEngine` to `LpsEngine::new`.

Shaders are already converted in Phase 3; nothing to do there.

**Out of scope:** the WASM emit dead-code cleanup (M4b).

### Code Organization Reminders

(See Phase 0.)

### Sub-agent Reminders

(See Phase 0.)

### Implementation Details

Files:

- `lp-core/lp-engine/src/gfx/native_jit.rs` (rewrite)
- (read-only) `lp-shader/lpvm-native/src/rt_jit/engine.rs`
- (read-only) `lp-shader/lp-shader/src/engine.rs`

Pattern (mirror of Phase 3):

```rust
pub struct NativeJitGraphics {
    engine: lp_shader::LpsEngine<lpvm_native::NativeJitEngine>,
    // builtin_table no longer needs to be a separate field; NativeJitEngine owns it
}

impl NativeJitGraphics {
    pub fn new() -> Self {
        lps_builtins::ensure_builtins_referenced();
        let mut table = lpvm_native::BuiltinTable::new();
        table.populate();
        let backend = lpvm_native::NativeJitEngine::new(
            alloc::sync::Arc::new(table),
            lpvm_native::NativeCompileOptions::default(),
        );
        Self { engine: lp_shader::LpsEngine::new(backend) }
    }
}

impl LpGraphics for NativeJitGraphics {
    fn compile_shader(
        &self,
        source: &str,
        options: &ShaderCompileOptions,
    ) -> Result<Box<dyn LpShader>, Error> {
        let cfg = options.to_compiler_config();
        let px = self.engine.compile_px(
            source,
            lps_shared::TextureStorageFormat::Rgba16Unorm,
            &cfg,
        )?;
        Ok(Box::new(NativeJitShader { px }))
    }
    fn backend_name(&self) -> &'static str { "native-jit" }

    fn alloc_output_buffer(&self, width: u32, height: u32)
        -> Result<lp_shader::LpsTextureBuf, Error>
    {
        self.engine.alloc_texture(width, height,
            lps_shared::TextureStorageFormat::Rgba16Unorm)
            .map_err(|e| Error::Other { message: format!("alloc texture: {e:?}") })
    }
}

struct NativeJitShader { px: lp_shader::LpsPxShader }

impl LpShader for NativeJitShader {
    fn render(&mut self, buf: &mut lp_shader::LpsTextureBuf, time: f32)
        -> Result<(), Error>
    {
        // Same uniforms construction as cranelift path; consider sharing a helper.
        // ...
        self.px.render_frame(&uniforms, buf).map_err(|e| Error::Other {
            message: format!("render_frame: {e}"),
        })
    }
    fn has_render(&self) -> bool { true }
}
```

Delete `render_native_jit_direct`.

### Validate

```
cargo check -p lp-engine --features native-jit
cargo test  -p lp-engine
just build-rv32          # per AGENTS.md, this is the rv32 firmware build
```

(Sub-agent: if `just build-rv32` is unavailable in the environment,
report the failure but do not attempt to fix the build environment;
the supervisor will validate locally.)

## Phase 5 — Cleanup & validation  [sub-agent: supervised]

### Scope of phase

Workspace-wide cleanup pass.

### Code Organization Reminders

(See Phase 0.)

### Sub-agent Reminders

(See Phase 0.)

### Implementation Details

1. **Grep the diff** for temporary code, debug prints, `eprintln!`,
   `dbg!`, ad-hoc `println!`, and remove them.
2. **Remove TODOs introduced during the plan**, except the explicitly
   surfaced ones (`max_errors`, `texture_data state`).
3. **Audit deprecation warnings**: every `#[deprecated]` use of
   `lp_shared::Texture` outside the crate that defines it should be
   silenced with `#[allow(deprecated)]` only inside the texture's
   own crate self-references (impl blocks, internal helpers). External
   warnings are intentional — count them and add a sentence in the
   "Decisions" section listing the residual call sites and which
   future plan removes them.
4. **Update `docs/roadmaps/2026-04-16-lp-shader-textures/m4a-pixel-loop-migration.md`**:
   tick the deliverables, link to this plan file, note "see plan-done".
5. **Move plan file** `docs/plans/2026-04-19-m4a-pixel-loop-migration.md`
   → `docs/plans-done/2026-04-19-m4a-pixel-loop-migration.md`.
6. **Append `# Decisions for future reference`** to the plan file with
   any decisions captured during the run that future-you would
   benefit from. Likely entries:
   - Buffer ownership moved from TextureRuntime to ShaderRuntime
     (revisit when adding non-shader texture sources, e.g. image
     loads).
   - Pixel-center vs pixel-corner coord change.
   - `lpvm-native` config-drop bug fix path.
   - Per-call `CompilerConfig` via new `LpvmEngine::compile_with_config`
     trait method (revisit if engines gain richer per-call options).

### Validate

```
cargo check --workspace
cargo test  --workspace
cargo fmt   --all
cargo clippy --workspace -- -D warnings  # or whatever the project uses
just build-rv32                            # firmware
```

All must pass cleanly. No warnings (other than the intentional
external `Texture` deprecations, which should be enumerated in the
decisions section).

# Notes (raw / unresolved)

- **Pixel-center vs pixel-corner:** confirmed no current shader cares
  about the difference.
- **WASM emit special-case:** `lpvm-wasm/src/emit/mod.rs` has a
  legacy-signature special-case (`vec4 render(vec2, vec2, float)`) for
  the old direct-call path. Becomes dead code after M4a but is left
  in place; M4b removes it during the host-backend swap.
- **`noise.fx` shader and `lpfx-cpu`:** out of scope for M4a. Kept on
  legacy signature; M4c handles it.
- **`max_errors` field on `ShaderCompileOptions`:** keep as-is with
  existing `// TODO` in the front-end. Out of scope.

# Decisions for future reference

Captured during the M4a implementation pass (Phase 0 → Phase 5):

- **GLSL signature: chose option 3, not option 1.** The roadmap recommended
  the bootstrap-wrapper approach (synthesise `__px_render(vec2 pos)` that
  calls the legacy 3-arg `render`). In implementation we instead migrated
  every engine-target GLSL source to the new contract directly:
  `layout(binding = N) uniform …;` plus `vec4 render(vec2 pos)`.
  Reason: simpler runtime, no front-end magic, matches the post-M1
  destination anyway. Affected files:
  - `examples/basic/src/rainbow.shader/main.glsl`
  - `examples/basic2/src/rainbow.shader/main.glsl`
  - `examples/fast/src/simple.shader/main.glsl`
  - `examples/mem-profile/src/rainbow.shader/main.glsl`
  - `lp-app/web-demo/www/rainbow-default.glsl`
  - Embedded strings in: `lp-core/lp-shared/src/project/builder.rs`,
    `lp-core/lp-server/src/template.rs`,
    `lp-cli/src/commands/create/project.rs`,
    `lp-core/lp-engine/tests/scene_update.rs`.
  Bare `uniform vec2 outputSize; uniform float time;` declarations were
  not picked up by the uniform reflection path; explicit
  `layout(binding = …)` qualifiers are required. If the reflection layer
  later auto-assigns bindings, the explicit qualifiers can be dropped.
- **Buffer ownership moved from `TextureRuntime` to `ShaderRuntime`.**
  `TextureRuntime` no longer owns pixel storage — it just carries
  config (width, height, format) and exposes empty `state.texture_data`
  with `TODO(M4a): texture_data state should come from upstream shader's
  buffer`. `ShaderRuntime` owns an `Option<lp_shader::LpsTextureBuf>`
  and (re)allocates it via `LpGraphics::alloc_output_buffer` whenever
  the bound texture's config or owner changes. Revisit when adding
  non-shader texture sources (image loads, video, etc.) — those will
  need somewhere else to live, probably back on the texture node.
- **Multi-shader-per-texture owner concept.** When more than one shader
  targets the same texture handle, exactly one of them owns the
  physical `LpsTextureBuf`; the others write through a shared mutable
  reference handed out by `RenderContext::get_target_texture_pixels_mut`.
  Owner = highest `render_order`, with `NodeHandle::as_i32` as the
  tie-breaker. See `texture_output_owner_handle` in
  `lp-core/lp-engine/src/project/runtime.rs` and
  `NodeInitContext::texture_output_buffer_owner`.
- **Pixel-center vs pixel-corner.** Confirmed no current shader cares
  about the difference; `LpsPxShader::render_frame` interprets `pos` as
  whatever the synthetic `__render_texture_<format>` produces (M2). No
  shim added.
- **`lpvm-native` config-drop bug fix path.** Phase 0 changed
  `compile_module_jit` to take `&NativeCompileOptions` directly, and
  `NativeJitEngine::compile` now forwards `&self.options` instead of
  re-flattening to `(float_mode, alloc_trace)`. New regression test:
  `lpvm-native/src/compile.rs::compile_module_respects_q32_mul_mode_in_emitted_code`
  (asserts saturating-mul lowers to a builtin call, wrapping-mul
  inlines `mul/mulh`, so emitted code differs).
- **`lpvm-native::opt::fold_immediates` is now loop-aware.** During
  Phase 5 validation, `fw-tests::test_scene_render_fw_emu` regressed
  on `lpvm-native::rt_jit`: the synthesised `__render_texture_rgba16`
  loop wrote every iteration to pixel 0. Root cause: `fold_immediates`
  walked `vinsts` linearly, recording `IConst32` values; an
  `IConst32 v_pxoff = 0` defined *before* the per-pixel loop survived
  in its `vreg_const` map and was folded into the in-loop
  `AluRRR Add tex_ptr, v_pxoff` use as `addi …, 0`, even though the
  same vreg was being mutated each iteration by `IaddImm` inside the
  loop. Fix: pre-compute, per loop region, the bitset of vregs def'd
  anywhere in `[header_idx ..= backedge_idx]`; refuse to use a recorded
  constant when the use is inside a loop where the source vreg has any
  in-loop def. New regression test:
  `lpvm-native/src/opt.rs::tests::no_fold_loop_invariant_constant_when_vreg_mutated_in_loop`.
  This was a latent bug in the optimizer, not specific to M4a — but
  M4a's synthesised per-pixel loop was the first hot path to hit the
  pattern.
- **Per-call `CompilerConfig` via new `LpvmEngine::compile_with_config`
  trait method.** Default impl falls back to `compile()`; `Cranelift`
  and `NativeJit` engines override and merge the supplied
  `lpir::CompilerConfig` into their construction-time `CompileOptions`
  / `NativeCompileOptions`. `LpsEngine::compile_px` gained a
  `&lpir::CompilerConfig` parameter (19 test call sites updated).
  Revisit if engines grow richer per-call options — at that point the
  trait method will likely take an options struct rather than just
  `CompilerConfig`.
- **`unsafe impl Send + Sync` for `LpsTextureBuf` and `LpsPxShader`.**
  Required because `LpShader: Send + Sync` (engine graph trait bound).
  `LpsTextureBuf` contains raw pointers via `LpvmBuffer`;
  `LpsPxShader` contains `RefCell` for internal mutability. Justified
  in code comments by the single-threaded execution model of the
  engine's render loop. If the engine ever goes multi-threaded across
  shaders, revisit and either gate behind an explicit
  thread-confinement type or add real synchronisation.
- **`#[allow(deprecated)] on `lp_shared::Texture` self-references.**
  All `#[allow(deprecated)]` annotations live inside `lp-shared` itself
  (impl blocks and the crate-root re-export). Zero external warnings
  remain because no consumer outside `lp-shared` still references
  `Texture` — `lpfx-cpu` uses its own `CpuTexture`, and `lp-engine`
  routes everything through `lp_shader::LpsTextureBuf`. Per the plan,
  external warnings would have been intentional; the count is just
  zero. The deprecated `Texture` itself still has internal callers
  (its own unit tests in `lp-shared/src/util/texture.rs`) and will be
  removed entirely in a follow-up cleanup once those tests are either
  ported or retired.
- **Phase 4 collapsed into Phase 3.** The native-jit `gfx` migration
  was originally scoped as Phase 4 to keep the diff small, but in
  practice the Phase 3 trait change to `LpShader::render(&mut
  LpsTextureBuf, …)` would have required temporary shim plumbing
  inside `gfx/native_jit.rs`. Doing the full migration there at the
  same time was both shorter and avoided a throwaway intermediate. RV32
  cross-target check (`cargo check -p lp-engine --no-default-features
  --features native-jit --target riscv32imac-unknown-none-elf`) passes
  cleanly.
- **`q32 options: …` log line is intentionally `info!`.** The
  per-shader-compile `log::info!` in
  `lp-core/lp-engine/src/nodes/shader/runtime.rs` was kept at info
  level on user request — it's a quick way to spot wrong glsl_opts in
  the field. The Phase 0 unit test covers correctness; the log is for
  ops, not for tests. If shader compiles ever become hot enough that
  one info-line per compile is noise, demote to `trace`.
- **Shared `build_uniforms` helper.** `lp-engine/src/gfx/uniforms.rs`
  hosts the `outputSize` + `time` `LpsValueF32::Struct` builder used
  by both Cranelift and NativeJIT wrappers. Add fields here when new
  engine-managed uniforms appear (e.g. frame counter, audio data).
