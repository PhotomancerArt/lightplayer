// test run

// Constructor-typed coordinates: `ivec2(vec2)` / `vec2(ivec2)` conversion
// constructors are valid GLSL and must be accepted by both frontends
// (rv32lpn exercises lps-glsl; the other targets exercise naga/lps-frontend).
// Regression: lps-frontend used to type naga `As` casts as scalar and reject
// `texelFetch(tex, ivec2(pos), 0)` with "coordinate must be ivec2".

// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=clamp shape=2d

// texture-data: inputColor 2x1 rgba16unorm
//   0.125,0.25,0.375,1.0 1.0,0.75,0.5,0.25

uniform sampler2D inputColor;

float fetch_r_via_vec2_conversion() {
    vec2 pos = vec2(1.6, 0.4);
    // ivec2(vec2) truncates toward zero -> texel (1, 0)
    return texelFetch(inputColor, ivec2(pos), 0).r;
}

vec4 fetch_texel0_via_vec2_conversion() {
    vec2 pos = vec2(0.9, 0.0);
    return texelFetch(inputColor, ivec2(pos), 0);
}

vec4 sample_via_ivec2_conversion() {
    // vec2(ivec2) -> (0.75, 0.5): center of texel (1, 0)
    return texture(inputColor, vec2(ivec2(3, 1)) * vec2(0.25, 0.5));
}

// run: fetch_r_via_vec2_conversion() ~= 1.0 (tolerance: 0.0002)
// run: fetch_texel0_via_vec2_conversion() ~= vec4(0.125, 0.25, 0.375, 1.0) (tolerance: 0.0002)
// run: sample_via_ivec2_conversion() ~= vec4(1.0, 0.75, 0.5, 0.25) (tolerance: 0.0003)
