// test run

// v0 texelFetch policy: clamp out-of-range integer coordinates to edge texels (memory-safe).

// texture-spec: t format=rgba16unorm filter=nearest wrap=clamp shape=2d

// Row-major 2×2:
// y=0: (0,0) red-orange   (1,0) muted green
// y=1: (0,1) muted blue    (1,1) bright white

// texture-data: t 2x2 rgba16unorm
//   1.0,0.2,0.1,1.0  0.0,0.7,0.2,1.0
//   0.0,0.2,0.95,1.0  1.0,1.0,1.0,1.0

uniform sampler2D t;

vec4 corner_negative_clamp() {
    return texelFetch(t, ivec2(-999, -999), 0);
}

vec4 corner_large_clamp() {
    return texelFetch(t, ivec2(128, 256), 0);
}

vec4 interior() {
    return texelFetch(t, ivec2(1, 1), 0);
}

vec4 clamp_x_only() {
    return texelFetch(t, ivec2(100, 0), 0);
}

// Negative → (0,0)
// run: corner_negative_clamp() ~= vec4(1.0, 0.2, 0.1, 1.0) (tolerance: 0.0003)

// Oversized → last texel along each axis → (1,1)
// run: corner_large_clamp() ~= vec4(1.0, 1.0, 1.0, 1.0) (tolerance: 0.0003)

// In-range sanity
// run: interior() ~= vec4(1.0, 1.0, 1.0, 1.0) (tolerance: 0.0003)

// y in range but x past edge → clamps x to width-1 → (1,0)
// run: clamp_x_only() ~= vec4(0.0, 0.7, 0.2, 1.0) (tolerance: 0.0003)
