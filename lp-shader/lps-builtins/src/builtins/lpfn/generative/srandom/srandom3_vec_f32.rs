//! 3D Signed Random function returning Vec3Q32 (float implementation - stub).

use crate::builtins::lpfn::generative::srandom::srandom3_vec_q32::__lp_lpfn_srandom3_vec_q32;
use lps_q32::q32::Q32;

#[lpfn_impl_macro::lpfn_impl(f32, "vec3 lpfn_srandom3_vec(vec3 p, uint seed)")]
#[allow(
    clippy::not_unsafe_ptr_arg_deref,
    reason = "builtin C ABI writes vec3 through caller-provided out-pointer"
)]
#[unsafe(no_mangle)]
pub extern "C" fn __lp_lpfn_srandom3_vec_f32(out: *mut f32, x: f32, y: f32, z: f32, seed: u32) {
    let x_q32 = Q32::from_f32_wrapping(x);
    let y_q32 = Q32::from_f32_wrapping(y);
    let z_q32 = Q32::from_f32_wrapping(z);

    let mut result_q32 = [0i32; 3];
    __lp_lpfn_srandom3_vec_q32(
        result_q32.as_mut_ptr(),
        x_q32.to_fixed(),
        y_q32.to_fixed(),
        z_q32.to_fixed(),
        seed,
    );

    unsafe {
        *out = Q32::from_fixed(result_q32[0]).to_f32();
        *out.add(1) = Q32::from_fixed(result_q32[1]).to_f32();
        *out.add(2) = Q32::from_fixed(result_q32[2]).to_f32();
    }
}
