// test run

// Nearest + repeat; sample with u outside [0,1] → wrapped column.

// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=repeat shape=2d

// texture-data: inputColor 2x1 rgba16unorm
//   1.0,0.0,0.0,1.0  0.0,1.0,0.0,1.0

uniform sampler2D inputColor;

vec4 wraps_to_left() {
    return texture(inputColor, vec2(1.25, 0.5));
}

// u=1.25, w=2 → repeat → same texel as column 0
// run: wraps_to_left() ~= vec4(1.0, 0.0, 0.0, 1.0) (tolerance: 0.0003)
