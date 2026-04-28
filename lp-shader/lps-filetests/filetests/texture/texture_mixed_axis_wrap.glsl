// test run

// Independent wrap: repeat on X, clamp on Y — sample with u outside [0,1] and v < 0.

// texture-spec: inputColor format=rgba16unorm filter=nearest wrap_x=repeat wrap_y=clamp shape=2d

// 2×2:
// texture-data: inputColor 2x2 rgba16unorm
//   1.0,0.2,0.1,1.0  0.0,0.7,0.2,1.0
//   0.0,0.2,0.95,1.0  1.0,1.0,1.0,1.0

uniform sampler2D inputColor;

vec4 repeat_x_clamp_y() {
    return texture(inputColor, vec2(1.25, -0.15));
}

// x repeats to column 0; y clamps to row 0 → same as texelFetch corner (0,0)
// run: repeat_x_clamp_y() ~= vec4(1.0, 0.2, 0.1, 1.0) (tolerance: 0.0004)
