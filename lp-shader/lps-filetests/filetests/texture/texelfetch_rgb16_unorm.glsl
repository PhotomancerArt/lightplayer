// test run

// Default targets: rv32n.q32, rv32c.q32, wasm.q32 - aligned Load16U per channel via texelFetch.

// Rgb16Unorm loads three stored channels; alpha must widen to 1.0.

// texture-spec: t format=rgb16unorm filter=nearest wrap=clamp shape=2d

// texture-data: t 2x1 rgb16unorm
//   0.2,0.4,0.6  0.1,0.3,0.5

uniform sampler2D t;

vec4 fetch_a() {
    return texelFetch(t, ivec2(0, 0), 0);
}

float fetch_alpha_a() {
    return texelFetch(t, ivec2(0, 0), 0).a;
}

vec4 fetch_b() {
    return texelFetch(t, ivec2(1, 0), 0);
}

// run: fetch_a() ~= vec4(0.2, 0.4, 0.6, 1.0) (tolerance: 0.0003)
// run: fetch_alpha_a() ~= 1.0 (tolerance: 0.0002)
// run: fetch_b() ~= vec4(0.1, 0.3, 0.5, 1.0) (tolerance: 0.0003)
