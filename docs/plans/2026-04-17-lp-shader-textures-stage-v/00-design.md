# M2.0 — `render_frame` via Synthetic `__render_texture`: Design

## Scope

Move the per-pixel render loop into LPIR. Replace `LpsPxShader::render_frame`'s
current "set uniforms only" stub with one direct call into a synthesised
`__render_texture[_<format>]` function that contains the full nested y/x
pixel loop, calls `render(vec2 pos)` per pixel, converts Q32 → unorm16,
and writes channels into the texture buffer via `Store16`.

Three changes happen together:

1. **A new dedicated trait method** `LpvmInstance::call_render_texture`
   provides a typed, fast, allocation-free hot path bypassing the
   generic `call_q32` slow path.
2. **A backend-agnostic synthesis routine** in `lp-shader/src/synth/`
   produces `__render_texture_<format>` LPIR per `(LpsModuleSig,
   TextureStorageFormat, render_fn_index)`, specialised per format so
   per-channel offsets and bytes-per-pixel are constant-folded.
3. **`LpsPxShader` is type-erased** behind `Box<dyn PxShaderBackend>`,
   so the public type is monomorphic and `LpsEngine::compile_px`
   stops leaking `<E::Module>` to callers.

Closes M2.0 in [`docs/roadmaps/2026-04-16-lp-shader-textures/m2-render-frame.md`](../../roadmaps/2026-04-16-lp-shader-textures/m2-render-frame.md).

## Background

All open design questions are tracked in [`00-notes.md`](./00-notes.md);
the highlights:

- **Per-pixel coords** (Q1) — pixel centre in Q32:
  `pos = ((x << 16) + 32768, (y << 16) + 32768)`.
- **Globals reset per pixel** (Q2 + Q3) — emit `Memcpy(globals,
  snapshot, size)` only when globals exist *and* are mutated, gated
  on a per-module `has_global_mutations` flag.
- **One specialised function per format** (Q4) — naming
  `__render_texture_r16` / `_rgb16` / `_rgba16`; channel count, bpp,
  per-channel offsets baked into the IR.
- **Pointer ABI** (Q8) — sidestepped via the dedicated trait method;
  `IrType::Pointer` semantics in LPIR are unchanged (host-width on
  `lpvm-cranelift` JIT only; 32-bit on Wasmtime host, RV32, emu, and
  browser — see roadmap “Host execution backend”).
- **`__shader_init` stays once-per-instance** (Q11.5) — desktop-GLSL
  uniform-dependent global initialisers are technically a latent bug
  but unreachable through the GPU pipeline (Naga → WGSL); future work
  will tighten the Naga subset.
- **Synthetic functions** (Q11) — exposed via `LpsModuleSig.functions`
  with a new `LpsFnKind::{UserDefined, Synthetic}` discriminant on
  `LpsFnSig`; consumers filter via `kind == UserDefined` if needed.

## File Structure

```
lp-shader/
├── lps-shared/src/
│   └── sig.rs                    # NEW: LpsFnKind enum, kind field on LpsFnSig
│
├── lps-frontend/src/
│   └── lower.rs                  # UPDATE: stamp kind on every emitted LpsFnSig
│
├── lpvm/src/
│   └── instance.rs               # UPDATE: add call_render_texture trait method
│
├── lpvm-cranelift/src/
│   └── lpvm_instance.rs          # UPDATE: impl call_render_texture (JIT; Phase 2 smoke + legacy)
│
├── lpvm-native/src/
│   ├── rt_jit/instance.rs        # UPDATE: impl call_render_texture (RV32 JIT path)
│   └── rt_emu/instance.rs        # UPDATE: impl call_render_texture (RV32 emu path)
│
├── lpvm-emu/src/
│   └── instance.rs               # UPDATE: impl call_render_texture (interp path)
│
├── lpvm-wasm/src/
│   ├── rt_wasmtime/instance.rs   # UPDATE: impl call_render_texture (wasmtime)
│   └── rt_browser/instance.rs    # UPDATE: impl call_render_texture (browser)
│
└── lp-shader/src/
    ├── synth/
    │   ├── mod.rs                # NEW
    │   └── render_texture.rs     # NEW: backend-agnostic synthesis
    ├── px_shader.rs              # UPDATE: Box<dyn PxShaderBackend>, render_frame impl
    ├── engine.rs                 # UPDATE: synthesise after lower(), before compile()
    └── tests/
        └── render_frame_pixels.rs # NEW: 4 format-correctness tests
```

## Conceptual Architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│                       LpsEngine::compile_px                          │
│                                                                      │
│  glsl ──► lps_frontend::lower ──► (LpirModule, LpsModuleSig)         │
│                                          │                           │
│                                          ▼                           │
│                       synth::render_texture::synthesise              │
│                       (adds __render_texture_<fmt> to LpirModule;    │
│                        adds matching LpsFnSig{kind:Synthetic} to     │
│                        LpsModuleSig.functions)                       │
│                                          │                           │
│                                          ▼                           │
│                              engine.compile(ir, meta)                │
│                              (each backend lowers Pointer: i64 on     │
│                               lpvm-cranelift JIT only; i32 on         │
│                               Wasmtime host and all other backends)   │
│                                          │                           │
│                                          ▼                           │
│                              LpsPxShader::new                        │
│                              (validates __render_texture_<fmt>       │
│                               present + signature shape)             │
└──────────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────────┐
│                     LpsPxShader::render_frame                        │
│                                                                      │
│  apply_uniforms(uniforms)                                            │
│       │                                                              │
│       ▼                                                              │
│  inner.call_render_texture(name, tex.buffer_mut(), w, h)             │
│       │                                                              │
│       ▼                                                              │
│  PxShaderBackend::call_render_texture                                │
│       │                                                              │
│       ▼                                                              │
│  LpvmInstance::call_render_texture(name, &mut LpvmBuffer, w, h)      │
│       │                                                              │
│       ├─ first call: resolve entry, validate sig, cache              │
│       └─ subsequent calls: cache hit                                 │
│       │                                                              │
│       ▼                                                              │
│  one machine call → compiled loop → per-pixel render() → Store16 ×N  │
└──────────────────────────────────────────────────────────────────────┘
```

## Main Components

### `LpsFnKind` (lps-shared)

```rust
/// Whether a function in `LpsModuleSig.functions` is user-authored
/// or synthesised by the toolchain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LpsFnKind {
    /// Lowered from user GLSL.
    UserDefined,
    /// Synthesised by lps-frontend or lp-shader (e.g. `__shader_init`,
    /// `__render_texture_<format>`). Always begins with `__`.
    Synthetic,
}

pub struct LpsFnSig {
    pub name: String,
    pub return_type: LpsType,
    pub parameters: Vec<FnParam>,
    pub kind: LpsFnKind,           // NEW
}
```

### `LpvmInstance::call_render_texture` (lpvm)

```rust
pub trait LpvmInstance {
    type Error: core::fmt::Display;

    // existing: call, call_q32, set_uniform, set_uniform_q32, ...

    /// Hot path: invoke the synthesised `__render_texture[_<format>]`
    /// entry by name. Resolves the entry on first call, caches it
    /// internally, and reuses the cache on subsequent calls.
    ///
    /// Validates signature shape `(Pointer, I32, I32) -> ()` on the
    /// first lookup. Returns the backend's existing `Error` type for
    /// missing symbol, signature mismatch, or guest trap.
    fn call_render_texture(
        &mut self,
        fn_name: &str,
        texture: &mut LpvmBuffer,
        width: u32,
        height: u32,
    ) -> Result<(), Self::Error>;
}
```

Per-backend cache shape (each backend keeps it in its own field;
typically `Option<(String, ResolvedEntry)>`, since the same instance
will only ever render with one format under normal usage):

| Backend                     | `ResolvedEntry`                |
|-----------------------------|--------------------------------|
| `lpvm-cranelift`            | `*const u8` (JIT'd fn ptr)     |
| `lpvm-native::rt_jit`       | `usize` (RV32 entry address)   |
| `lpvm-native::rt_emu`       | `usize` (entry pc into emu)    |
| `lpvm-emu`                  | `usize` (entry pc into emu)    |
| `lpvm-wasm::rt_wasmtime`    | `wasmtime::Func`               |
| `lpvm-wasm::rt_browser`     | `js_sys::Function` (or eq.)    |

The `Pointer` argument is extracted from `LpvmBuffer`:
- `lpvm-cranelift` JIT: `texture.native_ptr() as i64` (real 64-bit host ptr).
- Wasmtime, RV32, emu, browser: `texture.guest_base() as i32` (32-bit guest
  offset / linear-memory offset).

### Synthesis routine (lp-shader)

```rust
// lp-shader/src/synth/render_texture.rs

pub struct SynthRenderTexture<'a> {
    pub render_fn_index: usize,
    pub format: TextureStorageFormat,
    pub has_global_mutations: bool,   // gated reset; cheap query on lpir
}

pub fn synthesise(
    module: &mut LpirModule,
    meta: &mut LpsModuleSig,
    spec: SynthRenderTexture,
) -> Result<(), SynthError>;
```

- Pure transformation: appends one `IrFunction` to `module.functions`
  *and* the matching `LpsFnSig { kind: Synthetic, .. }` to
  `meta.functions` so the 1:1 zip backends rely on stays intact
  ([`lpvm-native/rt_jit/module.rs:63`](../../../lp-shader/lpvm-native/src/rt_jit/module.rs),
  [`lpvm-wasm/compile.rs:60-81`](../../../lp-shader/lpvm-wasm/src/compile.rs)).
- Function name: `__render_texture_<format_suffix>` where suffix is
  `r16` / `rgb16` / `rgba16`.
- Signature: `(tex_ptr: Pointer, width: I32, height: I32) -> Void`.
- Body shape (pseudo-LPIR; constants are baked, not loaded):

```text
fn __render_texture_<fmt>(tex_ptr: Pointer, width: I32, height: I32):
    // Hoisted constants (codegen rematerialises cheaply).
    BPP    = const                  // 2 / 6 / 8 depending on format
    Q_HALF = 32768                  // pixel-centre in Q32 (0x8000)
    Q_ONE  = 65536                  // 1.0 in Q32 (1 << 16)

    pos_y  = Q_HALF
    px_off = 0
    y      = 0
    loop {
        if y >= height: break

        pos_x = Q_HALF
        x     = 0
        loop {
            if x >= width: break

            // (only if has_global_mutations — gated per Q3)
            Memcpy(globals_addr, snapshot_addr, globals_size)

            color = render(pos_x, pos_y)   // direct Call (inliner deferred)

            // per-channel Q32 → unorm16 + Store16, unrolled, BPP baked
            Store16(tex_ptr, px_off + 0, q32_to_unorm16(color[0]))
            // ... up to CHANNELS-1

            // Incremental updates — no per-pixel multiplications.
            px_off += BPP
            pos_x  += Q_ONE
            x      += 1
        }

        pos_y += Q_ONE
        y     += 1
    }
```

#### Why incremental updates / nested loop

This is hot-path code. The naive shape (`pos_x = (x<<16)+Q_HALF;
px_off = y*width*BPP + x*BPP`) costs **2 multiplies per pixel** —
expensive on `lpvm-native`'s `rv32fa` target, which has no M
extension and emulates multiply in software (libcalls).

The shape above eliminates *every* multiplication from the inner
loop. `pos_x`, `pos_y`, `px_off` advance by constant additions;
the only mul is one-shot for `total_bytes` if synth chooses to
hoist that, which it doesn't need to (the nested cmps already
terminate correctly).

A single-flat-loop variant was considered (one termination cmp + a
`col == width` row-wrap branch per pixel). Rejected: it has
*more* branches per pixel than the nested form for any
`width ≥ 2`, with no compensating win. The nested loop is
structurally simpler and strictly fewer dynamic branches.

### Q32 → unorm16 conversion

The naive `(v_clamped * 65535) >> 16` overflows i32 when
`v_clamped == 65536` (`65536 * 65535 ≈ 4.29e9 > i32::MAX`). Use the
algebraically equivalent overflow-free form:

```text
v_clamped = max(0, min(value, 65536))
u16_val   = v_clamped - (v_clamped >> 16)   // exact for v in [0, 65536]
```

Proof: `(v * 65535) / 65536 = v - v/65536`. For non-negative i32,
`v >> 16 == v / 65536`. No multiplication, no overflow, no helper
function — emitted inline per channel.

### `LpsPxShader` refactor (lp-shader)

```rust
pub struct LpsPxShader {
    inner: Box<dyn PxShaderBackend>,
    output_format: TextureStorageFormat,
    meta: LpsModuleSig,
    /// Format-specific synthesised entry, e.g. "__render_texture_rgba16".
    render_fn_name: String,
    /// Index of `render` in `meta.functions` (preserved from compile_px).
    render_fn_index: usize,
}

trait PxShaderBackend {
    fn call_render_texture(
        &mut self,
        name: &str,
        texture: &mut LpvmBuffer,
        width: u32,
        height: u32,
    ) -> Result<(), LpsError>;

    fn set_uniform(&mut self, path: &str, value: &LpsValueF32)
        -> Result<(), LpsError>;
}
```

`LpsPxShader::new` validates the synthesised render function's
presence and signature in `meta()` *before* the first frame —
push-time validation that would otherwise need a separate `lookup`
trait method.

`LpsEngine::compile_px` now returns `LpsPxShader` (not
`LpsPxShader<E::Module>`), so the public surface is monomorphic. The
backend type is erased at the boundary via a small adapter struct that
owns `(M, M::Instance)` and implements `PxShaderBackend`.

### `render_frame`

```rust
pub fn render_frame(
    &self,
    uniforms: &LpsValueF32,
    tex: &mut LpsTextureBuf,
) -> Result<(), LpsError> {
    self.apply_uniforms(uniforms)?;
    let w = tex.width();
    let h = tex.height();
    let mut buf = tex.buffer();
    self.inner
        .borrow_mut()
        .call_render_texture(&self.render_fn_name, &mut buf, w, h)
}
```

One direct v-call → one trait method on the instance → cache hit on
the resolved entry → one machine call into compiled guest code. After the
first frame: no string-table lookup, no allocations, no marshalling.

## Phases

Six phases; sequential except where noted:

1. **`LpsFnKind` on `LpsFnSig`** ([`01-lps-fn-kind.md`](./01-lps-fn-kind.md))
   — Standalone prep. Lands first so subsequent phases stamp the
   right kind from the moment they create new functions.
2. **LPVM trait extension** ([`02-lpvm-trait-extension.md`](./02-lpvm-trait-extension.md))
   — Adds `call_render_texture` to `LpvmInstance`. Implements on all
   six backends. Includes the lpvm-cranelift JIT smoke test (Q10 #5).
3. **Synthesis routine** ([`03-render-texture-synth.md`](./03-render-texture-synth.md))
   — `lp-shader/src/synth/render_texture.rs`. Adds the inliner
   sanity assertion (Q7).
4. **`LpsPxShader` refactor** ([`04-px-shader-refactor.md`](./04-px-shader-refactor.md))
   — `Box<dyn>`, warmup validation, `render_frame` wires it all.
   Compiles end-to-end against all backends; pre-existing tests
   (e.g. `render_frame_sets_uniforms`) keep passing.
5. **End-to-end pixel tests** ([`05-end-to-end-pixel-tests.md`](./05-end-to-end-pixel-tests.md))
   — Four format-correctness tests on **`lpvm-wasm` / `WasmLpvmEngine`
   (Wasmtime)** — the supported host-execution path for `lp-shader`
   tests and the direction for `lp-cli` / authoring tools. The
   `lpvm-native` rt_emu path and browser wasm are validated downstream
   by `fw-emu` / `fw-wasm`-based integration tests once M2.0 is threaded
   through `lpfx` / `lp-engine` (M4). M2.0 ships with runtime correctness
   validated on the Wasmtime path; `lpvm-cranelift` is covered by the
   Phase 2 handwritten JIT smoke (trait shape) plus `cargo build`; the
   other backends are validated for compile-time correctness via
   `cargo build`.
6. **Cleanup + workspace validation** ([`06-cleanup-validation.md`](./06-cleanup-validation.md))
   — Doc/comment sweep, `cargo build --all-features`, targeted test
   runs.

## Out of Scope

- Texture **reads** (`sampler2D`, `texelFetch`) — separate milestone (M3).
- Consumer migration (lpfx-cpu, lp-engine) — separate milestone (M4).
- Multi-target rendering (multiple output textures).
- Compute-shader-style dispatch.
- Additional pixel formats beyond R16Unorm / Rgb16Unorm / Rgba16Unorm.
- LPIR inliner integration — Phase 3's sanity assertion will need
  inversion when the inliner lands; that's tracked separately.
- Compile-time const-expression evaluation for global initialisers
  (would obsolete `__shader_init`) — future optimisation, orthogonal.
- Naga subset enforcement (rejecting non-const-expr module-scope
  initialisers in lps-frontend) — future correctness hardening.

## Validation

```bash
cargo check -p lps-shared -p lps-frontend
cargo check -p lpvm
cargo build --workspace --all-features

cargo test -p lps-shared
cargo test -p lpvm
cargo test -p lpvm-cranelift            # Phase 2 JIT smoke (handwritten LPIR)
cargo test -p lp-shader                 # Phase 5 format tests (full pipeline; Wasmtime default)
```

End-to-end correctness lands in Phase 5; per-phase validation lives
in each phase doc.

## Implementation notes

- **Host execution path:** `lp-shader` Phase 5 format tests and the
  supported host direction use **`lpvm-wasm` / Wasmtime**
  (`WasmLpvmEngine`), not the in-process `lpvm-cranelift` JIT. The
  Cranelift JIT crate remains in-tree and is covered by the Phase 2
  handwritten smoke test; it is **deprecated for new host work** while
  `lp-engine` / `lpfx-cpu` still depend on it (migration is M4).
- **Per-instance render-texture cache:** backends keep a small optional
  cache (typically `Option<…>`) keyed by the synthesised function name,
  populated on first `call_render_texture` — matching the “resolve once,
  reuse” intent in the design body.
