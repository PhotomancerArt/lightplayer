// test run

// R16 UNORM sampling expands to vec4 (R, 0, 0, 1).

// texture-spec: inputColor format=r16unorm filter=nearest wrap=clamp shape=2d

// texture-data: inputColor 2x1 r16unorm
//   0.75  0.25

uniform sampler2D inputColor;

vec4 sample_left_texel() {
    return texture(inputColor, vec2(0.125, 0.5));
}

// run: sample_left_texel() ~= vec4(0.75, 0.0, 0.0, 1.0) (tolerance: 0.0004)
