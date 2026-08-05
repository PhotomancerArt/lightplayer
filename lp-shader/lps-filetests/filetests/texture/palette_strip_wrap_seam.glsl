// test run

// The palette strip's seam.
//
// Palette bindings use `wrap=repeat` on X deliberately
// (`lpc_engine::nodes::shader::shader_node::palette_texture_specs`): a shader
// may scroll `u` without clamping, and a gradient authored to wrap joins
// itself. The consequence is that `u = 0` is NOT the first texel — it is the
// midpoint between the last texel and the first, because both centers are
// half a texel away from the seam. A palette author sees that as "the ramp
// closes"; this file is what makes it a contract rather than a surprise.

// texture-spec: palette format=rgba16unorm filter=linear wrap=repeat shape=height-one

// texture-data: palette 4x1 rgba16unorm
//   1.0,0.0,0.0,1.0  0.0,1.0,0.0,1.0  0.0,0.0,1.0,1.0  1.0,1.0,1.0,1.0

uniform sampler2D palette;

// The seam: half of texel 3 (white) and half of texel 0 (red).
vec4 at_seam() {
    return texture(palette, vec2(0.0, 0.0));
}

// A full turn past the seam is the same sample.
vec4 one_turn_later() {
    return texture(palette, vec2(1.0, 0.0));
}

// Scrolling past the end wraps rather than clamping: u = 1.125 is texel 0's
// center again.
vec4 scrolled_past_the_end() {
    return texture(palette, vec2(1.125, 0.0));
}

// And backwards, for a shader subtracting a phase.
vec4 scrolled_before_the_start() {
    return texture(palette, vec2(-0.125, 0.0));
}

// interp.f32: no guest memory to bind texture fixtures into
// wgpu.f32: texture fixtures are not bound through the GPU registry yet
// @unsupported(interp.f32)
// @unsupported(wgpu.f32)
// run: at_seam() ~= vec4(1.0, 0.5, 0.5, 1.0) (tolerance: 0.005)
// @unsupported(interp.f32)
// @unsupported(wgpu.f32)
// run: one_turn_later() ~= vec4(1.0, 0.5, 0.5, 1.0) (tolerance: 0.005)
// @unsupported(interp.f32)
// @unsupported(wgpu.f32)
// run: scrolled_past_the_end() ~= vec4(1.0, 0.0, 0.0, 1.0) (tolerance: 0.005)
// @unsupported(interp.f32)
// @unsupported(wgpu.f32)
// run: scrolled_before_the_start() ~= vec4(1.0, 1.0, 1.0, 1.0) (tolerance: 0.005)
