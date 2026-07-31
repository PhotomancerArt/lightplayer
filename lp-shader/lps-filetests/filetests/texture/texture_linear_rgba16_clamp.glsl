// test run

// Linear + clamp; bilinear blend (not exactly one texel).

// texture-spec: inputColor format=rgba16unorm filter=linear wrap=clamp shape=2d

// texture-data: inputColor 2x1 rgba16unorm
//   1.0,0.0,0.0,1.0  0.0,1.0,0.0,1.0

uniform sampler2D inputColor;

vec4 center_blend() {
    return texture(inputColor, vec2(0.5, 0.5));
}

// u=0.5 → halfway between columns → R and G both ~0.5
// interp.f32: no guest memory to bind texture fixtures into
// wgpu.f32: texture fixtures are not bound through the GPU registry yet
// @unsupported(interp.f32)
// @unsupported(wgpu.f32)
// wasm.f32: builtin import has no f32 implementation — only Q32 builtin ids
// resolve, so the import cannot be lowered in f32 mode. Unblocks with M5.
// @unimplemented(wasm.f32)
// run: center_blend() ~= vec4(0.5, 0.5, 0.0, 1.0) (tolerance: 0.004)
