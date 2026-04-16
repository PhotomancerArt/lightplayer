# M1 — CPU Rendering Path (lpfx-cpu)

Wire `FxEngine` → `FxModule` → `FxInstance` for the CPU/WASM path via lpvm.

## Goal

An `FxModule` loaded from disk can be compiled and rendered on the CPU.
The rendering path goes through `lps-frontend` → LPIR → lpvm (either
cranelift on host or WASM in browser). Inputs from `fx.toml` map to
shader uniforms.

## Deliverables

### `lpfx/lpfx-cpu` crate

Implements the `FxEngine` / `FxInstance` types backed by the lpvm stack.

```rust
pub struct CpuFxEngine {
    // Wraps an LpvmEngine (CraneliftEngine, BrowserLpvmEngine, etc.)
}

impl CpuFxEngine {
    pub fn instantiate(
        &self,
        module: &FxModule,
        resolution: (u32, u32),
    ) -> Result<CpuFxInstance, Error>;
}

pub struct CpuFxInstance {
    // Holds: LpvmModule + LpvmInstance, output Texture, input defs + current values
}

impl CpuFxInstance {
    pub fn set_input(&mut self, name: &str, value: FxValue) -> Result<(), Error>;
    pub fn render(&mut self, time: f32) -> Result<(), Error>;
    pub fn output(&self) -> &Texture;
}
```

### Input → uniform mapping

The `instantiate` step:
1. Reads `main.glsl` from the module
2. Compiles via `lps_frontend::compile` + `lower`
3. Maps each `[input.*]` entry to a uniform in the compiled shader
   (matched by name — the GLSL must declare `uniform float speed;` etc.)
4. Sets all uniforms to their `default` values from the manifest
5. Allocates an RGBA16 output texture at the requested resolution

`set_input` converts `FxValue` to the appropriate uniform representation
and writes it via `LpvmInstance::write_vmctx_bytes`.

`render` runs the globals/uniforms lifecycle (init, snapshot, pixel loop)
like `CraneliftShader::render` does today.

### GLSL contract

The effect's `main.glsl` must provide:

```glsl
vec4 render(vec2 fragCoord, vec2 outputSize, float time)
```

`fragCoord`, `outputSize`, and `time` are provided by the runtime (not
uniforms — they are function arguments, same as today). All other inputs
are declared as `uniform` and matched by name to `[input.*]` entries.

### Host test

Run `rainbow-noise.fx` on the host via cranelift:

```rust
let engine = CpuFxEngine::new_cranelift();
let module = FxModule::load("lpfx/effects/rainbow-noise.fx")?;
let mut instance = engine.instantiate(&module, (64, 64))?;
instance.set_input("speed", FxValue::F32(2.0))?;
instance.render(1.0)?;
let texture = instance.output();
// verify non-black, correct dimensions
```

## Dependencies

- M0 (scaffold + manifest + effect on disk)
- Existing `lps-frontend`, `lpvm`, `lpvm-cranelift`, `lpvm-wasm`
- Existing uniforms/globals infrastructure

## Validation

```bash
cargo test -p lpfx-cpu
cargo check -p lpfx-cpu
# Also: the rainbow-noise.fx module renders non-trivial output
```
