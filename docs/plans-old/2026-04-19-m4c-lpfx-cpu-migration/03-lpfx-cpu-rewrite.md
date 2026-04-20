# Phase 3 — `lpfx-cpu` rewrite onto `lp-shader`

`[sub-agent: yes, parallel: -]`

## Scope of phase

Rewrite `lpfx-cpu` as a thin shim over `lp-shader`'s high-level API,
mirroring the M4b shape of `lp-engine`'s `gfx::Graphics`:

- **`Cargo.toml`**: drop the `cranelift` Cargo feature and the
  `lpvm-cranelift` / `lps-frontend` / `lpir` deps. Add a `std` Cargo
  feature (default-on) forwarded to `lpfx`, `lp-shader`, `lps-shared`,
  `lpvm`. Wire `lp-shader` and `lps-shared` deps. Target-gate the LPVM
  backend dep: `lpvm-native` on RV32, `lpvm-wasm` everywhere else.
  Pull in `lps-builtins` unconditionally (matches `lp-engine`'s
  `Cargo.toml` — only the RV32 path actually uses it).
- **`src/backend.rs`** (NEW): defines a `LpvmBackend` type alias and a
  `new_backend()` constructor that does the per-target dispatch. Same
  shape as `lp-engine/src/gfx/{host,wasm_guest,native_jit}.rs` but
  collapsed into one file (lpfx-cpu has no `LpGraphics` trait to
  satisfy — the indirection isn't earned).
- **`src/lib.rs`**: rewrite `CpuFxEngine` and `CpuFxInstance`. The
  engine holds one `LpsEngine<LpvmBackend>` (constructed in
  `CpuFxEngine::new`) and a `BTreeMap<TextureId, LpsTextureBuf>`.
  `instantiate` calls `engine.compile_px`, validates inputs, and
  hands the resulting `LpsPxShader` + the removed `LpsTextureBuf` to
  the new `CpuFxInstance`. `CpuFxInstance::render(&FxRenderInputs)`
  builds the uniforms `LpsValueF32::Struct` and calls
  `LpsPxShader::render_frame`.
- **`src/compile.rs`**: shrink to just the input ↔ uniform validator.
  Drop `compile_glsl` and `CompiledEffect`.
- **`src/render_cranelift.rs`**: DELETE.
- **Tests**: update the two existing tests to use `defaults_from_manifest`
  + `FxRenderInputs` instead of `set_input` + `render(time)`. Read pixels
  via `TextureBuffer::data()`.

## Out of scope

- Anything in `lpfx/lpfx/`. Phase 2 owns that.
- Anything in `examples/`. Phase 1 owns the `noise.fx` GLSL.
- Performance benchmarking. Bench work is its own milestone.
- Support for output formats other than `Rgba16Unorm` (the
  `engine.compile_px(…)` call hard-codes
  `TextureStorageFormat::Rgba16Unorm` to match
  `FxEngine::create_texture`'s shape).
- Adding a generic `CpuFxEngine<E>` escape hatch. Per `00-notes.md`
  Q2: target-arch dispatch only; the generic escape hatch is
  additive and we don't need it now.
- Adding a `CpuFxEngine::from_engine(LpsEngine<E>)` constructor. Per
  Q5: defer until the eventual `lp-engine`-consumes-`lpfx-cpu`
  integration actually needs it.

## Code organization reminders

- One concept per file. Backend dispatch in `src/backend.rs`. Engine
  + instance in `src/lib.rs`. Validator in `src/compile.rs`. Don't
  collapse them.
- Public types / traits at the top of each file; helpers at the
  bottom. Tests live in their own `mod tests` at the bottom of
  `lib.rs`.
- Keep `#![no_std]` + `extern crate alloc` at the top of
  `src/lib.rs`. All code uses `core` + `alloc` only.
- No `TODO` comments. Anything left for future work belongs in
  `00-notes.md`'s "future work" section, not in code.
- No `#[allow(...)]` additions to mask warnings.

## Sub-agent reminders

- Do **not** commit. Phase 4 commits the whole plan as one unit.
- Do **not** expand scope. Phase 1 owns `examples/noise.fx`. Phase 2
  owns `lpfx/lpfx/`. This phase owns `lpfx/lpfx-cpu/` only. If you
  notice a needed change in either of the other two scopes, **stop
  and report**.
- Do **not** suppress warnings or add `#[allow(...)]`. Fix the
  underlying issue.
- Do **not** disable, `#[ignore]`, or weaken the two existing tests
  (`noise_fx_renders_nonblack`, `noise_fx_default_inputs`). They
  must continue to pass after the rewrite, with the test bodies
  updated to the new API shape.
- Do **not** introduce a generic `CpuFxEngine<E>` or
  `CpuFxInstance<E>`. The engine and instance are concrete types
  parameterised only by the target-arch-selected `LpvmBackend` alias.
- If anything is ambiguous or blocked, **stop and report**.
- Report back: files changed (added / edited / deleted), validation
  output, and the final diff of `src/lib.rs`.

## Implementation details

### Reference for backend dispatch shape

`lp-core/lp-engine/src/gfx/host.rs`,
`lp-core/lp-engine/src/gfx/wasm_guest.rs`, and
`lp-core/lp-engine/src/gfx/native_jit.rs` are the M4b precedents.
`lpfx-cpu/src/backend.rs` is a flatter version of the same idea —
no `LpGraphics` trait, just a type alias + constructor.

### File 1 — `lpfx/lpfx-cpu/Cargo.toml`

Replace the existing file body with:

```toml
[package]
name = "lpfx-cpu"
description = "CPU rendering backend for lpfx effect modules (no_std + alloc)"
version.workspace = true
authors.workspace = true
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[lints]
workspace = true

[features]
default = ["std"]
std = [
    "lpfx/std",
    "lp-shader/std",
    "lps-shared/std",
]

[dependencies]
lpfx       = { path = "../lpfx",                          default-features = false }
lp-shader  = { path = "../../lp-shader/lp-shader",        default-features = false }
lps-shared = { path = "../../lp-shader/lps-shared",       default-features = false }
lpvm       = { path = "../../lp-shader/lpvm",             default-features = false }
lpir       = { path = "../../lp-shader/lpir",             default-features = false }
lps-builtins = { path = "../../lp-shader/lps-builtins",   default-features = false }

# Backend selection: exactly one LPVM engine is wired into `CpuFxEngine`,
# chosen by target architecture rather than a Cargo feature.
#   - riscv32       → lpvm-native (rt_jit)        — bare-metal firmware
#   - everything else → lpvm-wasm (rt_wasmtime/rt_browser) — host or wasm32 guest
[target.'cfg(target_arch = "riscv32")'.dependencies]
lpvm-native = { path = "../../lp-shader/lpvm-native", default-features = false }

[target.'cfg(not(target_arch = "riscv32"))'.dependencies]
lpvm-wasm = { path = "../../lp-shader/lpvm-wasm", default-features = false }
```

Notes:

- `lpvm-cranelift` removed from deps entirely.
- `lps-frontend` removed: `LpsEngine::compile_px` does the
  GLSL→naga→LPIR pipeline internally.
- `lpir` kept because `LpsEngine::compile_px` takes a
  `&CompilerConfig` from `lpir`.
- `lps-builtins` is unconditional (matches `lp-engine`); only the
  RV32 backend uses its `ensure_builtins_referenced()` /
  `BuiltinTable`. On host the symbols sit unused, same trade-off
  `lp-engine` made.
- `std` feature only forwards downstream `std` knobs that exist;
  `lpvm`, `lpir`, `lps-builtins`, `lpvm-native`, `lpvm-wasm`
  currently don't expose `std` features (see their `Cargo.toml`s)
  so we don't list them. Mirror exactly what `lp-engine`'s `std`
  feature forwards (`lp-shared/std`, here translated to the lpfx
  parent + lp-shader + lps-shared trio).

### File 2 (NEW) — `lpfx/lpfx-cpu/src/backend.rs`

Target-arch-dispatched LPVM backend selection. Mirrors `lp-engine`'s
three backend modules collapsed into one file (lpfx-cpu has no
`LpGraphics` trait equivalent — `LpsEngine` is the only abstraction
needed).

```rust
//! LPVM backend selection by target architecture.
//!
//! | Target                                  | Backend                            |
//! |-----------------------------------------|------------------------------------|
//! | `cfg(target_arch = "riscv32")`          | `lpvm-native::rt_jit`              |
//! | `cfg(target_arch = "wasm32")`           | `lpvm-wasm::rt_browser`            |
//! | catchall (host)                         | `lpvm-wasm::rt_wasmtime`           |
//!
//! Picked at compile time. There is no Cargo feature for selecting
//! a backend; the dep blocks in `Cargo.toml` already gate which
//! crate is in scope. `LpvmBackend` is the type alias users see;
//! `new_backend()` is the constructor. Both are crate-internal.

#[cfg(target_arch = "riscv32")]
mod imp {
    use alloc::sync::Arc;

    use lpvm_native::{BuiltinTable, NativeCompileOptions, NativeJitEngine};

    pub type LpvmBackend = NativeJitEngine;

    pub fn new_backend() -> LpvmBackend {
        lps_builtins::ensure_builtins_referenced();
        let mut table = BuiltinTable::new();
        table.populate();
        NativeJitEngine::new(Arc::new(table), NativeCompileOptions::default())
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use lpvm_wasm::WasmOptions;
    use lpvm_wasm::rt_browser::BrowserLpvmEngine;

    pub type LpvmBackend = BrowserLpvmEngine;

    pub fn new_backend() -> LpvmBackend {
        BrowserLpvmEngine::new(WasmOptions::default())
            .expect("BrowserLpvmEngine::new with default WasmOptions")
    }
}

#[cfg(not(any(target_arch = "riscv32", target_arch = "wasm32")))]
mod imp {
    use lpvm_wasm::WasmOptions;
    use lpvm_wasm::rt_wasmtime::WasmLpvmEngine;

    pub type LpvmBackend = WasmLpvmEngine;

    pub fn new_backend() -> LpvmBackend {
        WasmLpvmEngine::new(WasmOptions::default())
            .expect("WasmLpvmEngine::new with default WasmOptions")
    }
}

pub(crate) use imp::{LpvmBackend, new_backend};
```

Notes:

- The three `mod imp` arms are mutually exclusive; only one compiles
  per target. Imports inside each arm are scoped, so unused-import
  warnings don't trigger on the inactive ones.
- The `LpvmBackend` alias and `new_backend()` are `pub(crate)`; the
  rest of `lpfx-cpu` consumes them via
  `use crate::backend::{LpvmBackend, new_backend};`.

### File 3 (DELETE) — `lpfx/lpfx-cpu/src/render_cranelift.rs`

Delete the file outright.

### File 4 — `lpfx/lpfx-cpu/src/compile.rs`

Drop `compile_glsl` and the `CompiledEffect` struct. Keep
`validate_inputs`. Thin imports.

```rust
//! Manifest input ↔ shader uniform validation.

use alloc::format;
use alloc::string::String;

use lps_shared::LpsModuleSig;
use lps_shared::path_resolve::LpsTypePathExt;

use lpfx::FxManifest;

/// Ensures each `[input.X]` has a corresponding uniform field `input_X` in the shader metadata.
pub(crate) fn validate_inputs(
    manifest: &FxManifest,
    meta: &LpsModuleSig,
) -> Result<(), String> {
    let uniforms = meta.uniforms_type.as_ref();
    for (name, _def) in &manifest.inputs {
        let uniform_name = format!("input_{name}");
        if let Some(ut) = uniforms {
            if ut.type_at_path(&uniform_name).is_err() {
                return Err(format!(
                    "manifest input `{name}` has no matching uniform `{uniform_name}` in shader"
                ));
            }
        } else {
            return Err(format!(
                "shader has no uniforms but manifest declares input `{name}`"
            ));
        }
    }
    Ok(())
}
```

`validate_inputs` was previously `pub`; demote to `pub(crate)`
because nothing outside `lpfx-cpu` calls it. (The roadmap mentions
keeping it as an `lpfx-cpu`-internal helper; this is the cleanup
step.)

The `lpir` import goes away with `compile_glsl`; the `lpvm` import
goes away too. The remaining file is small and self-contained.

### File 5 — `lpfx/lpfx-cpu/src/lib.rs`

Full rewrite. Replace the existing file body with:

```rust
#![no_std]
//! CPU backend for [`lpfx`] modules: compiles GLSL via `lp-shader`
//! and renders into [`LpsTextureBuf`] outputs.
//!
//! Backend selection is target-driven (see [`backend`]). One
//! [`CpuFxEngine`] owns one [`LpsEngine`], which owns one
//! [`lpvm::LpvmEngine`] — engines are 1-to-1-to-1.

extern crate alloc;

mod backend;
mod compile;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lp_shader::{LpsEngine, LpsPxShader, LpsTextureBuf};
use lpir::CompilerConfig;
use lps_shared::lps_value_f32::LpsValueF32;
use lps_shared::{TextureBuffer, TextureStorageFormat};

use lpfx::engine::{FxEngine, FxInstance};
use lpfx::texture::TextureId;
use lpfx::{FxModule, FxRenderInputs, FxValue};

use crate::backend::{LpvmBackend, new_backend};

/// CPU effect engine: one shared LPVM backend, one shared
/// `LpsEngine`, one bump-allocated texture pool.
///
/// Every [`Self::create_texture`] and [`Self::instantiate`] call
/// reuses the same underlying `LpsEngine`, so all textures and
/// compiled shaders share a single LPVM memory pool. The pool is a
/// bump allocator on host (M4b pre-grows wasmtime linear memory to
/// 64 MiB ≈ 8M `Rgba16Unorm` pixels); textures are not freed
/// individually — only dropping the whole `CpuFxEngine` reclaims
/// the pool.
pub struct CpuFxEngine {
    engine: LpsEngine<LpvmBackend>,
    textures: BTreeMap<TextureId, LpsTextureBuf>,
    next_id: u32,
}

impl CpuFxEngine {
    /// New engine with the target-arch-default LPVM backend.
    #[must_use]
    pub fn new() -> Self {
        Self {
            engine: LpsEngine::new(new_backend()),
            textures: BTreeMap::new(),
            next_id: 0,
        }
    }
}

impl Default for CpuFxEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// One running effect instance.
///
/// Holds a compiled [`LpsPxShader`] and the [`LpsTextureBuf`] it
/// renders into. Inputs are supplied per-render via
/// [`FxRenderInputs`]; nothing is cached on the instance between
/// renders.
pub struct CpuFxInstance {
    /// Manifest input name → shader uniform name (`speed` → `input_speed`).
    input_names: BTreeMap<String, String>,
    output: LpsTextureBuf,
    px: LpsPxShader,
}

impl CpuFxInstance {
    /// Read-only access to the output buffer (use [`TextureBuffer::data`]
    /// for raw bytes).
    #[must_use]
    pub fn output(&self) -> &LpsTextureBuf {
        &self.output
    }
}

impl FxEngine for CpuFxEngine {
    type Instance = CpuFxInstance;
    type Error = String;

    fn create_texture(&mut self, width: u32, height: u32) -> TextureId {
        let id = TextureId::from_raw(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        let buf = self
            .engine
            .alloc_texture(width, height, TextureStorageFormat::Rgba16Unorm)
            .expect("alloc Rgba16Unorm texture from shared LPVM memory");
        self.textures.insert(id, buf);
        id
    }

    fn instantiate(
        &mut self,
        module: &FxModule,
        output: TextureId,
    ) -> Result<Self::Instance, Self::Error> {
        let out_tex = self
            .textures
            .remove(&output)
            .ok_or_else(|| format!("unknown texture id {}", output.raw()))?;

        let cfg = CompilerConfig::default();
        let px = self
            .engine
            .compile_px(&module.glsl_source, TextureStorageFormat::Rgba16Unorm, &cfg)
            .map_err(|e| format!("compile_px: {e}"))?;

        compile::validate_inputs(&module.manifest, px.meta())?;

        let mut input_names = BTreeMap::new();
        for key in module.manifest.inputs.keys() {
            input_names.insert(key.clone(), format!("input_{key}"));
        }

        Ok(CpuFxInstance {
            input_names,
            output: out_tex,
            px,
        })
    }
}

impl FxInstance for CpuFxInstance {
    type Error = String;

    fn render(&mut self, inputs: &FxRenderInputs<'_>) -> Result<(), Self::Error> {
        let width = self.output.width();
        let height = self.output.height();

        let mut fields: Vec<(String, LpsValueF32)> =
            Vec::with_capacity(2 + inputs.inputs.len());
        fields.push((
            String::from("outputSize"),
            LpsValueF32::Vec2([width as f32, height as f32]),
        ));
        fields.push((String::from("time"), LpsValueF32::F32(inputs.time)));

        for (name, value) in inputs.inputs {
            let uniform_name = self
                .input_names
                .get(*name)
                .ok_or_else(|| format!("unknown input: {name}"))?;
            fields.push((uniform_name.clone(), fx_value_to_lps(value)));
        }

        let uniforms = LpsValueF32::Struct {
            name: None,
            fields,
        };

        self.px
            .render_frame(&uniforms, &mut self.output)
            .map_err(|e| format!("render_frame: {e}"))
    }
}

fn fx_value_to_lps(value: &FxValue) -> LpsValueF32 {
    match value {
        FxValue::F32(v) => LpsValueF32::F32(*v),
        FxValue::I32(v) => LpsValueF32::I32(*v),
        FxValue::Bool(v) => LpsValueF32::Bool(*v),
        FxValue::Vec3(v) => LpsValueF32::Vec3(*v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpfx::{FxModule, defaults_from_manifest};

    const NOISE_FX_TOML: &str = include_str!("../../../examples/noise.fx/fx.toml");
    const NOISE_FX_GLSL: &str = include_str!("../../../examples/noise.fx/main.glsl");

    /// Read pixel `(x, y)` of an Rgba16Unorm texture as four `u16`s
    /// (little-endian). Test-only readback helper.
    fn pixel_u16(buf: &LpsTextureBuf, x: u32, y: u32) -> [u16; 4] {
        let bpp = buf.format().bytes_per_pixel();
        let offset = ((y as usize) * (buf.width() as usize) + x as usize) * bpp;
        let bytes = &buf.data()[offset..offset + bpp];
        [
            u16::from_le_bytes([bytes[0], bytes[1]]),
            u16::from_le_bytes([bytes[2], bytes[3]]),
            u16::from_le_bytes([bytes[4], bytes[5]]),
            u16::from_le_bytes([bytes[6], bytes[7]]),
        ]
    }

    #[test]
    fn noise_fx_renders_nonblack() {
        let module =
            FxModule::from_sources(NOISE_FX_TOML, NOISE_FX_GLSL).expect("parse fx module");

        let mut engine = CpuFxEngine::new();
        let tex = engine.create_texture(64, 64);
        let mut instance = engine.instantiate(&module, tex).expect("instantiate");

        // Seed defaults, then overlay the user-driven `speed`.
        let mut defaults = defaults_from_manifest(&module.manifest);
        for (name, value) in defaults.iter_mut() {
            if name == "speed" {
                *value = FxValue::F32(2.0);
            }
        }
        let inputs: alloc::vec::Vec<(&str, FxValue)> = defaults
            .iter()
            .map(|(n, v)| (n.as_str(), v.clone()))
            .collect();
        let render_inputs = FxRenderInputs {
            time: 1.0,
            inputs: &inputs,
        };

        instance.render(&render_inputs).expect("render");

        let output = instance.output();
        assert_eq!(output.width(), 64);
        assert_eq!(output.height(), 64);

        let mut nonzero = 0u32;
        for y in 0..64 {
            for x in 0..64 {
                let px = pixel_u16(output, x, y);
                if px[0] > 0 || px[1] > 0 || px[2] > 0 {
                    nonzero += 1;
                }
            }
        }
        assert!(
            nonzero > 100,
            "expected many non-black pixels, got {nonzero}"
        );
    }

    #[test]
    fn noise_fx_default_inputs() {
        let module =
            FxModule::from_sources(NOISE_FX_TOML, NOISE_FX_GLSL).expect("parse fx module");

        let mut engine = CpuFxEngine::new();
        let tex = engine.create_texture(16, 16);
        let mut instance = engine.instantiate(&module, tex).expect("instantiate");

        let defaults = defaults_from_manifest(&module.manifest);
        let inputs: alloc::vec::Vec<(&str, FxValue)> = defaults
            .iter()
            .map(|(n, v)| (n.as_str(), v.clone()))
            .collect();
        let render_inputs = FxRenderInputs {
            time: 0.0,
            inputs: &inputs,
        };
        instance.render(&render_inputs).expect("render with defaults");

        let center = pixel_u16(instance.output(), 8, 8);
        assert!(center[3] > 0, "alpha should be non-zero from render()");
    }
}
```

Notes on the rewrite:

- `instantiate` no longer applies manifest defaults — the caller does
  via `defaults_from_manifest()`. This matches the per-render uniform
  rebuild design (Q6).
- The `CpuFxEngine::texture` / `texture_mut` accessor methods are
  removed. They returned `&CpuTexture` / `&mut CpuTexture` and were
  only useful for callers writing pixels into the input pool from
  outside the trait API. With `LpsTextureBuf` (shared-memory,
  guest-addressable) that's a different thing entirely; if a future
  caller needs read access to a pool texture, add it then.
- `CpuFxInstance::output()` returns `&LpsTextureBuf` (concrete, not
  `&dyn TextureBuffer`) — callers can `as &dyn TextureBuffer` on
  their side if they want the trait.
- The `pixel_u16` helper is `#[cfg(test)]` inside the test mod.
  It's the test's compatibility shim with the now-removed
  `CpuTexture::pixel_u16`.
- The `Vec::with_capacity(2 + inputs.inputs.len())` uniforms struct
  build is the per-frame allocation Q6 calls out as "negligible vs
  the pixel loop".
- `inputs.inputs` is a `&[(&str, FxValue)]`; iterating and cloning
  each `FxValue` into `LpsValueF32` is a tiny per-call cost.

## Validate

```bash
cargo build -p lpfx-cpu
cargo test  -p lpfx-cpu
```

Both must pass with the rewritten code, the migrated `noise.fx`
GLSL (phase 1), and the reshaped `lpfx` parent (phase 2). The two
existing tests are the bar for the per-pixel pipeline working
end-to-end through `LpsEngine::compile_px` →
`LpsPxShader::render_frame` → `LpsTextureBuf` writeback.

If either test fails:

- A `compile_px: Validation(...)` error means phase 1's GLSL
  migration didn't take (e.g. the file still has the legacy 3-arg
  signature). Verify `examples/noise.fx/main.glsl` matches the
  phase 1 target shape; if it doesn't, **stop and report**.
- A `render_frame: missing uniform field 'X'` error means the
  uniforms struct is missing a member declared in the shader.
  Verify `defaults_from_manifest` covers every input in
  `noise.fx/fx.toml` (it does — every input has a `default = …`),
  and that the `outputSize` + `time` fields are in the struct.
- A `compile_px: Compile(...)` or other backend error — **stop and
  report** the full message.

Don't attempt to validate other targets in this phase; phase 4 runs
the cross-target check matrix.

When reporting back, include:

- The list of files added / edited / deleted.
- The output of `cargo build -p lpfx-cpu` and `cargo test -p lpfx-cpu`.
- The final diff of `src/lib.rs`.
