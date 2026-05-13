// test run

// shape=height-one: 1D sampler path; varying uv.y must not affect the result.

// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=repeat shape=height-one

// texture-data: inputColor 4x1 rgba16unorm
//   1.0,0.0,0.0,1.0  0.0,1.0,0.0,1.0  0.0,0.0,1.0,1.0  1.0,1.0,1.0,1.0

uniform sampler2D inputColor;

vec4 low_v() {
    return texture(inputColor, vec2(0.125, 0.0));
}

vec4 high_v() {
    return texture(inputColor, vec2(0.125, 0.88));
}

// run: low_v() ~= vec4(1.0, 0.0, 0.0, 1.0) (tolerance: 0.0003)
// run: high_v() ~= vec4(1.0, 0.0, 0.0, 1.0) (tolerance: 0.0003)
