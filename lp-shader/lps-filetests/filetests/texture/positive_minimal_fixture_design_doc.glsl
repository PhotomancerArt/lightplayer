// test run
// Same texture-spec / texture-data / uniform as docs/design/lp-shader-texture-access.md (fixture excerpt).
// Minimal texelFetch on default targets (rv32n.q32, rv32c.q32, wasm.q32).

// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=clamp shape=2d

// texture-data: inputColor 2x1 rgba16unorm
//   1.0,0.0,0.0,1.0 0.0,1.0,0.0,1.0

uniform sampler2D inputColor;

vec4 texel_left() {
    return texelFetch(inputColor, ivec2(0, 0), 0);
}

vec4 texel_right() {
    return texelFetch(inputColor, ivec2(1, 0), 0);
}

// run: texel_left() ~= vec4(1.0, 0.0, 0.0, 1.0) (tolerance: 0.0002)
// run: texel_right() ~= vec4(0.0, 1.0, 0.0, 1.0) (tolerance: 0.0002)
