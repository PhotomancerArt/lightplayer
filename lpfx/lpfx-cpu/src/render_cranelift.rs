//! Q32 per-pixel loop via [`lpvm_cranelift::DirectCall`].

use lpvm_cranelift::{CraneliftInstance, CraneliftModule, DirectCall};

use lpfx::texture::CpuTexture;

pub(crate) struct CraneliftState {
    pub(crate) _module: CraneliftModule,
    pub(crate) instance: CraneliftInstance,
    pub(crate) direct_call: DirectCall,
}

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
            state.instance.reset_globals();

            let frag_coord_q32 = [(x as i32) * Q32_SCALE, (y as i32) * Q32_SCALE];
            let args = [
                frag_coord_q32[0],
                frag_coord_q32[1],
                output_size_q32[0],
                output_size_q32[1],
                time_q32,
            ];
            let mut rgba_q32 = [0i32; 4];
            unsafe {
                state
                    .direct_call
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
