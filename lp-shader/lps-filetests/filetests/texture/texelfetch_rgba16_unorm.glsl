// test run

// Default targets: rv32n.q32, rv32c.q32, wasm.q32 - multi-channel aligned Load16U/Rgba16Unorm path.

// Rgba16Unorm texelFetch: exact channel values for adjacent texels (fixture-backed checks).

// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=clamp shape=2d

// texture-data: inputColor 2x1 rgba16unorm
//   0.125,0.25,0.375,1.0 1.0,0.75,0.5,0.25

uniform sampler2D inputColor;

float sample_r_corner() {
    return texelFetch(inputColor, ivec2(0, 0), 0).r;
}

float sample_g_corner() {
    return texelFetch(inputColor, ivec2(0, 0), 0).g;
}

float sample_b_corner() {
    return texelFetch(inputColor, ivec2(0, 0), 0).b;
}

float sample_a_corner() {
    return texelFetch(inputColor, ivec2(0, 0), 0).a;
}

float sample_x1_r() {
    return texelFetch(inputColor, ivec2(1, 0), 0).r;
}

vec4 texel_far_t() {
    return texelFetch(inputColor, ivec2(1, 0), 0);
}

// run: sample_r_corner() ~= 0.125 (tolerance: 0.0002)
// run: sample_g_corner() ~= 0.25 (tolerance: 0.0002)
// run: sample_b_corner() ~= 0.375 (tolerance: 0.0002)
// run: sample_a_corner() ~= 1.0 (tolerance: 0.0002)
// run: sample_x1_r() ~= 1.0 (tolerance: 0.0002)
// run: texel_far_t() ~= vec4(1.0, 0.75, 0.5, 0.25) (tolerance: 0.0002)
