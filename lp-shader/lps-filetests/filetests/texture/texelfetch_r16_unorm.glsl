// test run

// Default filetest targets (see lps-filetests `DEFAULT_TARGETS`): rv32n.q32, rv32c.q32, wasm.q32.
// Exercises naturally aligned LPIR Load16U on 16-bit texture channels and vec4 fill rules.

// R16Unorm: single stored channel; G/B must fill 0 and A fills 1.0.

// texture-spec: t format=r16unorm filter=nearest wrap=clamp shape=2d

// texture-data: t 1x1 r16unorm
//   0.625

uniform sampler2D t;

vec4 fetch_center() {
    return texelFetch(t, ivec2(0, 0), 0);
}

float fetch_r() {
    return texelFetch(t, ivec2(0, 0), 0).r;
}

float fetch_g() {
    return texelFetch(t, ivec2(0, 0), 0).g;
}

float fetch_b() {
    return texelFetch(t, ivec2(0, 0), 0).b;
}

float fetch_a() {
    return texelFetch(t, ivec2(0, 0), 0).a;
}

// run: fetch_r() ~= 0.625 (tolerance: 0.0002)
// run: fetch_g() ~= 0.0 (tolerance: 0.0002)
// run: fetch_b() ~= 0.0 (tolerance: 0.0002)
// run: fetch_a() ~= 1.0 (tolerance: 0.0002)
// run: fetch_center() ~= vec4(0.625, 0.0, 0.0, 1.0) (tolerance: 0.00025)
