# M4c — Design

Roadmap milestone:
[`docs/roadmaps/2026-04-16-lp-shader-textures/m4c-lpfx-cpu-migration.md`](../../roadmaps/2026-04-16-lp-shader-textures/m4c-lpfx-cpu-migration.md)
Predecessor (merged):
[`docs/plans-old/2026-04-19-m4b-host-backend-swap/`](../../plans-old/2026-04-19-m4b-host-backend-swap/)
Decisions: see [`00-notes.md`](./00-notes.md) (Q1–Q8 all resolved).

## Goal

Migrate `lpfx-cpu` onto `lp-shader`'s high-level API (`LpsEngine` /
`LpsPxShader` / `LpsTextureBuf`), mirroring what M4a did for
`lp-engine`'s shader nodes and what M4b did for `lp-engine`'s
backend selection.

After M4c:

- `lpfx-cpu` is a thin shim over `lp-shader`. The hand-rolled per-pixel
  loop and Q32→unorm16 conversion are gone (deleted along with
  `render_cranelift.rs`).
- `lpfx-cpu` picks its LPVM backend by `cfg(target_arch = …)` — no
  `cranelift` Cargo feature, no `lpvm-cranelift` dep.
- `lpfx::texture` shrinks to just `TextureId`. `CpuTexture` and
  `TextureFormat` are deleted; the public API exposes `LpsTextureBuf`.
- The `FxInstance` trait is reshaped to take all uniforms per render
  call (`render(&FxRenderInputs)`); `set_input` is gone. Mirrors
  `LpsPxShader::render_frame`.
- `examples/noise.fx/main.glsl` is migrated to the new
  `render(vec2 pos)` + uniforms contract enforced by
  `LpsEngine::compile_px`.
- `lpfx-cpu` (and `lpfx`) keep `#![no_std]` + `extern crate alloc`,
  with a `std` Cargo feature forwarded through the dep tree exactly
  like `lp-engine` does — so RV32 firmware can still consume `lpfx-cpu`
  once `lp-engine` starts depending on it.

## Architecture

```
                  lpfx (parent, no_std + alloc, std feature)
                  ┌──────────────────────────────────────┐
                  │  FxModule, FxManifest, FxValue,      │
                  │  FxInputDef, TextureId               │
                  │  FxEngine trait                      │  ← create_texture(w, h)  (no format)
                  │  FxInstance trait                    │  ← render(&FxRenderInputs)
                  │  FxRenderInputs (NEW)                │  ← { time, inputs: &[(&str, FxValue)] }
                  │  defaults_from_manifest() helper     │  ← seed Vec<(String, FxValue)>
                  └──────────────────────────────────────┘
                                   ▲
                                   │ implements
                                   │
                  lpfx-cpu (no_std + alloc, std feature)
                  ┌──────────────────────────────────────┐
                  │  CpuFxEngine {                       │
                  │    engine: LpsEngine<LpvmBackend>,   │  ← single shared engine
                  │    textures: BTreeMap<               │
                  │      TextureId, LpsTextureBuf>,      │
                  │    next_id: u32,                     │
                  │  }                                   │
                  │                                      │
                  │  CpuFxInstance {                     │
                  │    input_names:                      │
                  │      BTreeMap<String, String>,       │  ← user "speed" → uniform "input_speed"
                  │    output: LpsTextureBuf,            │
                  │    px: LpsPxShader,                  │
                  │  }                                   │
                  └──────────────────────────────────────┘
                                   │
                                   │ uses
                                   ▼
                  lp-shader (LpsEngine, LpsPxShader, LpsTextureBuf)
                                   │
                                   ▼
                       LpvmBackend (target-arch alias, lpfx-cpu/src/backend.rs):

        ┌──────────────────────────┼──────────────────────────┐
cfg(target_arch = "riscv32")  catchall (host)         cfg(target_arch = "wasm32")
        ▼                          ▼                          ▼
  NativeJitEngine            WasmLpvmEngine            BrowserLpvmEngine
  (lpvm-native::rt_jit)      (lpvm-wasm::rt_wasmtime)  (lpvm-wasm::rt_browser)
```

Same per-target-arch shape M4b established for `lp_engine::Graphics`.

## Per-frame data flow

```
caller builds FxRenderInputs {
    time: 1.0,
    inputs: &[("speed", FxValue::F32(2.0)), ("zoom", FxValue::F32(3.0)), …]
}
        │
        ▼
CpuFxInstance::render(&inputs)
        │
        │  build LpsValueF32::Struct {
        │    fields: [
        │      ("outputSize", Vec2(w, h)),    ← from output texture (instance-bound)
        │      ("time",       F32(time)),
        │      ("input_speed", F32(2.0)),     ← from inputs slice via input_names map
        │      ("input_zoom",  F32(3.0)),
        │      …
        │    ]
        │  }
        ▼
LpsPxShader::render_frame(&uniforms, &mut output: LpsTextureBuf)
        │
        ▼
backend per-pixel loop writes Rgba16Unorm into the buffer
```

Uniforms are rebuilt per render call. Cost: a small `Vec` allocation
plus a single map walk over `input_names`. Negligible vs. the pixel
loop.

## File layout (target end-state)

```
lpfx/lpfx/Cargo.toml             ← add `std` feature (default-on)
lpfx/lpfx/src/lib.rs             ← drop CpuTexture / TextureFormat re-exports;
                                   add FxRenderInputs and defaults_from_manifest re-exports
lpfx/lpfx/src/engine.rs          ← reshape FxInstance trait (drop set_input,
                                   render takes &FxRenderInputs); drop format
                                   param from FxEngine::create_texture
lpfx/lpfx/src/render_inputs.rs   ← NEW: FxRenderInputs struct
lpfx/lpfx/src/defaults.rs        ← NEW: defaults_from_manifest() helper
lpfx/lpfx/src/texture.rs         ← shrink to just TextureId
                                   (delete CpuTexture, TextureFormat, their tests)

lpfx/lpfx-cpu/Cargo.toml         ← drop `cranelift` feature; add `std` feature;
                                   target-gated lpvm-native (rv32) / lpvm-wasm (rest);
                                   drop `lpvm-cranelift`, `lps-frontend`, `lpir` deps
                                   (lp-shader handles those)
lpfx/lpfx-cpu/src/lib.rs         ← rewrite: CpuFxEngine holds one LpsEngine<LpvmBackend>;
                                   CpuFxInstance holds LpsPxShader + LpsTextureBuf;
                                   instantiate() uses LpsEngine::compile_px;
                                   render() builds uniforms struct then calls render_frame
lpfx/lpfx-cpu/src/backend.rs     ← NEW: LpvmBackend type alias + new_backend()
                                   constructor, target-arch dispatched
lpfx/lpfx-cpu/src/compile.rs     ← shrink: keep validate_inputs only
                                   (drop compile_glsl, drop CompiledEffect)
lpfx/lpfx-cpu/src/render_cranelift.rs   ← DELETE

examples/noise.fx/main.glsl      ← migrate render(fragCoord, outputSize, time)
                                   → render(vec2 pos) with uniforms
```

No other crate in the workspace depends on `lpfx-cpu`, so the blast
radius is contained to `lpfx/lpfx/` + `lpfx/lpfx-cpu/` + the one
example shader.

## Phases

```
1. noise.fx GLSL migration                                        [sub-agent: yes,        parallel: 2]
2. lpfx parent reshape (FxRenderInputs, drop CpuTexture/Format)   [sub-agent: yes,        parallel: 1]
3. lpfx-cpu rewrite (Cargo + lib.rs + compile.rs + delete + tests) [sub-agent: yes,        parallel: -]
4. Cleanup, RV32 check, validation, summary                       [sub-agent: supervised, parallel: -]
```

Phases 1 and 2 share a parallel group: they touch disjoint files
(`examples/noise.fx/main.glsl` vs. `lpfx/lpfx/src/*` +
`lpfx/lpfx/Cargo.toml`) and both are preconditions for phase 3.

Phase 2's validation is limited to `cargo check -p lpfx`; the
parent crate's tests don't depend on `lpfx-cpu`. The full noise.fx
render test is exercised by phase 3 once both phases have landed.

Phase 3 must run after phases 1 + 2 — it consumes `FxRenderInputs`
and the migrated `noise.fx`.

Phase 4 is the cleanup + cross-target validation gate (host build,
host test, wasm32 check, rv32 check, summary, archive).

## Validation matrix (collected at the cleanup phase)

```bash
# Host build & test of lpfx parent + lpfx-cpu.
cargo build -p lpfx -p lpfx-cpu
cargo test  -p lpfx -p lpfx-cpu

# RV32 firmware path (no_std, lpvm-native).
cargo check -p lpfx-cpu --target riscv32imac-unknown-none-elf --no-default-features

# Wasm32 guest path (lpvm-wasm rt_browser).
cargo check -p lpfx-cpu --target wasm32-unknown-unknown
```

The RV32 check is the test for the `std`-feature plumbing: if any
of the dep edges accidentally drag in `std`, this fails.
