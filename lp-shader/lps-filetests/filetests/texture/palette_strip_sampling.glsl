// test run

// The palette strip contract, from the shader's side.
//
// `lpc-engine` bakes gradients into height-one `rgba16unorm` strips sampled
// `filter=linear wrap=repeat` (`lpc_engine::color::gradient_bake`), and texel
// `i` holds the gradient at `t = (i + 0.5) / WIDTH` — the texel CENTER. That
// convention only pays off if `texture(palette, vec2(u, 0))` returns the
// texel whose center is at `u`, which is what this pins, on both numeric
// tiers.

// texture-spec: palette format=rgba16unorm filter=linear wrap=repeat shape=height-one

// A 4-texel strip: red, green, blue, white. Centers at u = 0.125, 0.375,
// 0.625, 0.875.
// texture-data: palette 4x1 rgba16unorm
//   1.0,0.0,0.0,1.0  0.0,1.0,0.0,1.0  0.0,0.0,1.0,1.0  1.0,1.0,1.0,1.0

uniform sampler2D palette;

vec4 center_0() {
    return texture(palette, vec2(0.125, 0.0));
}

vec4 center_2() {
    return texture(palette, vec2(0.625, 0.0));
}

// Exactly between two centers is the even blend of their texels — the
// property that makes a 256-texel bake read as a smooth ramp rather than as
// 256 bands.
vec4 between_centers_1_2() {
    return texture(palette, vec2(0.5, 0.0));
}

// Height-one lowering ignores uv.y, so a shader may pass anything for it.
vec4 center_0_high_v() {
    return texture(palette, vec2(0.125, 0.77));
}

// interp.f32: no guest memory to bind texture fixtures into
// wgpu.f32: texture fixtures are not bound through the GPU registry yet
// @unsupported(interp.f32)
// @unsupported(wgpu.f32)
// run: center_0() ~= vec4(1.0, 0.0, 0.0, 1.0) (tolerance: 0.005)
// @unsupported(interp.f32)
// @unsupported(wgpu.f32)
// run: center_2() ~= vec4(0.0, 0.0, 1.0, 1.0) (tolerance: 0.005)
// @unsupported(interp.f32)
// @unsupported(wgpu.f32)
// run: between_centers_1_2() ~= vec4(0.0, 0.5, 0.5, 1.0) (tolerance: 0.005)
// @unsupported(interp.f32)
// @unsupported(wgpu.f32)
// run: center_0_high_v() ~= vec4(1.0, 0.0, 0.0, 1.0) (tolerance: 0.005)
