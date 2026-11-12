# Phase 4: Cranelift render loop + `set_input`

## Scope

Implement `render_cranelift.rs` with the Q32 per-pixel loop using
`DirectCall::call_i32_buf`. Wire `CpuFxInstance::render` and
`CpuFxInstance::set_input`.

## Code organization reminders

- One concept per file.
- Place public API first, pixel-loop helpers at the bottom.
- Keep related functionality grouped together.

## Implementation

### 4.1 `CpuFxInstance` definition

```rust
pub struct CpuFxInstance {
    // Compiled module metadata (for uniform encoding)
    meta: LpsModuleSig,
    // Input name -> uniform name mapping (name -> "input_{name}")
    input_names: BTreeMap<String, String>,
    // Backend-specific state:
    #[cfg(feature = "cranelift")]
    cranelift: CraneliftInstanceState,
    // Output texture handle (render writes here via engine ref)
    output: TextureId,
    output_width: u32,
    output_height: u32,
}
```

The instance needs a way to write to the output texture. Two approaches:
(a) the instance borrows the engine mutably during `render`, or
(b) the instance owns a clone/ref of the texture.

Since `FxInstance::render(&mut self, time)` has no engine parameter, the
instance should hold its own pixel buffer and the engine reads it back
after render. Alternatively, the instance holds a reference to the engine's
texture storage.

**Recommended approach:** `CpuFxInstance` holds the `CpuTexture` directly
(moved out of the engine's map during instantiation). The engine can
retrieve it via `instance.output()` or `instance.take_texture()`.

Simpler: just have `render` write into an internal `CpuTexture`:

```rust
pub struct CpuFxInstance {
    meta: LpsModuleSig,
    input_names: BTreeMap<String, String>,
    output: CpuTexture,
    #[cfg(feature = "cranelift")]
    cranelift: Option<CraneliftState>,
}

impl CpuFxInstance {
    pub fn output(&self) -> &CpuTexture { &self.output }
}
```

### 4.2 `render_cranelift.rs`

```rust
#[cfg(feature = "cranelift")]
use lpvm_cranelift::{CraneliftModule, CraneliftInstance, DirectCall};

pub(crate) struct CraneliftState {
    _module: CraneliftModule,
    instance: CraneliftInstance,
    direct_call: DirectCall,
}
```

The render function -- adapted from `lp-engine/src/gfx/cranelift.rs`:

```rust
pub(crate) fn render_cranelift(
    state: &mut CraneliftState,
    texture: &mut CpuTexture,
    time: f32,
) -> Result<(), alloc::string::String> {
    const Q32_SCALE: i32 = 65536;
    let width = texture.width();
    let height = texture.height();
    let time_q32 = (time * 65536.0 + 0.5) as i32;
    let output_size_q32 = [(width as i32) * Q32_SCALE, (height as i32) * Q32_SCALE];

    for y in 0..height {
        for x in 0..width {
            let frag_coord_q32 = [(x as i32) * Q32_SCALE, (y as i32) * Q32_SCALE];
            let args = [
                frag_coord_q32[0], frag_coord_q32[1],
                output_size_q32[0], output_size_q32[1],
                time_q32,
            ];
            let mut rgba_q32 = [0i32; 4];
            unsafe {
                state.direct_call
                    .call_i32_buf(state.instance.vmctx_ptr(), &args, &mut rgba_q32)
                    .map_err(|e| alloc::format!("render call failed: {e}"))?;
            }

            let clamp = |v: i32| v.max(0).min(Q32_SCALE);
            let r = ((clamp(rgba_q32[0]) as i64 * 65535) / Q32_SCALE as i64) as u16;
            let g = ((clamp(rgba_q32[1]) as i64 * 65535) / Q32_SCALE as i64) as u16;
            let b = ((clamp(rgba_q32[2]) as i64 * 65535) / Q32_SCALE as i64) as u16;
            let a = ((clamp(rgba_q32[3]) as i64 * 65535) / Q32_SCALE as i64) as u16;
            texture.set_pixel_u16(x, y, [r, g, b, a]);
        }
    }
    Ok(())
}
```

Note: `CraneliftInstance::vmctx_ptr()` is currently `fn vmctx_ptr(&self)`
(not `pub`). We may need to either make it `pub` or add a method on
`CraneliftInstance` that returns the pointer. Alternatively, the
cranelift render path uses `CraneliftModule::direct_call` +
`VmContextHeader::default()` like `lp-engine/cranelift.rs` does (which
does NOT use `CraneliftInstance` at all for the pixel loop -- it uses a
stack-allocated vmctx). Check which pattern supports uniforms.

**Key insight:** The `lp-engine` cranelift path uses a bare
`VmContextHeader::default()` with no instance -- so **uniforms are not
supported** on that path. For lpfx-cpu we need the `CraneliftInstance`
(which holds the vmctx buffer with globals + uniforms). We need the
instance's vmctx pointer passed to `DirectCall::call_i32_buf`.

If `vmctx_ptr()` is not public on `CraneliftInstance`, we'll need to
expose it (or add a `render`-style method that does the pixel loop
internally). **Check this during implementation.**

### 4.3 `set_input` implementation

```rust
impl FxInstance for CpuFxInstance {
    fn set_input(&mut self, name: &str, value: FxValue) -> Result<(), Self::Error> {
        let uniform_name = self.input_names.get(name)
            .ok_or_else(|| format!("unknown input: {name}"))?;
        let lps_val = fx_value_to_lps(&value);
        // Delegate to backend instance's set_uniform
        #[cfg(feature = "cranelift")]
        {
            self.cranelift.as_mut().unwrap()
                .instance
                .set_uniform(uniform_name, &lps_val)
                .map_err(|e| format!("set_uniform: {e}"))?;
        }
        Ok(())
    }
}
```

### 4.4 Apply manifest defaults at instantiation

After creating the instance, iterate `manifest.inputs` and call
`set_input` for each input that has a `default` value.

## Validate

```bash
cargo check -p lpfx-cpu
```

Integration test in phase 5.
