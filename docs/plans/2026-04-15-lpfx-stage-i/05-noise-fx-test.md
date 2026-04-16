# Phase 5: Update `noise.fx` + integration test

## Scope

Rename uniforms in `examples/noise.fx/main.glsl` to use `input_*` prefix.
Write an integration test that compiles + renders `noise.fx` via
`CpuFxEngine` (cranelift feature) and verifies non-trivial output.

## Code organization reminders

- One concept per file.
- Place tests alongside or in the crate's test module.
- Keep related functionality grouped together.

## Implementation

### 5.1 Update `examples/noise.fx/main.glsl`

Rename all uniforms:

```
speed         -> input_speed
zoom          -> input_zoom
noise_fn      -> input_noise_fn
palette       -> input_palette
cycle_palettes -> input_cycle_palettes
cycle_time_s  -> input_cycle_time_s
```

Both declarations and all usage sites in the shader.

### 5.2 Update `lpfx` M0 tests

The M0 test `noise_fx_compiles_in_lps_frontend` in `lpfx/lpfx/src/lib.rs`
uses `include_str!` for the GLSL -- it will pick up the renamed uniforms
automatically. Verify it still passes.

### 5.3 Integration test in `lpfx-cpu`

In `lpfx/lpfx-cpu/src/lib.rs` (or a `tests/` file):

```rust
#[cfg(all(test, feature = "cranelift"))]
mod tests {
    use super::*;
    use lpfx::FxModule;

    const NOISE_FX_TOML: &str = include_str!("../../../examples/noise.fx/fx.toml");
    const NOISE_FX_GLSL: &str = include_str!("../../../examples/noise.fx/main.glsl");

    #[test]
    fn noise_fx_renders_nonblack() {
        let module = FxModule::from_sources(NOISE_FX_TOML, NOISE_FX_GLSL)
            .expect("parse fx module");

        let mut engine = CpuFxEngine::new();
        let tex = engine.create_texture(64, 64, lpfx::TextureFormat::Rgba16);
        let mut instance = engine.instantiate(&module, tex).expect("instantiate");

        instance.set_input("speed", lpfx::FxValue::F32(2.0)).expect("set speed");
        instance.render(1.0).expect("render");

        let output = instance.output();
        assert_eq!(output.width(), 64);
        assert_eq!(output.height(), 64);

        // Verify non-trivial output: at least some non-zero pixels
        let mut nonzero = 0u32;
        for y in 0..64 {
            for x in 0..64 {
                let px = output.pixel_u16(x, y);
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
        let module = FxModule::from_sources(NOISE_FX_TOML, NOISE_FX_GLSL)
            .expect("parse fx module");

        let mut engine = CpuFxEngine::new();
        let tex = engine.create_texture(16, 16, lpfx::TextureFormat::Rgba16);
        let mut instance = engine.instantiate(&module, tex).expect("instantiate");

        // Render with just defaults (no set_input calls)
        instance.render(0.0).expect("render with defaults");

        let output = instance.output();
        // Should still produce non-trivial output from defaults
        let center = output.pixel_u16(8, 8);
        assert!(
            center[3] > 0,
            "alpha should be non-zero from render()"
        );
    }
}
```

### 5.4 Verify GLSL still compiles

```bash
cargo test -p lpfx  # M0 GLSL compile test
```

## Validate

```bash
cargo test -p lpfx
cargo test -p lpfx-cpu
cargo check
```

All tests pass. The noise effect renders real pixels.
