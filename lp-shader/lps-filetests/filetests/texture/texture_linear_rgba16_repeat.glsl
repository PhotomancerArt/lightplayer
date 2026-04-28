// test run

// Linear + repeat on X: fractional blend between texels k and k+1 with wrap (here columns 0 and 1).

// texture-spec: inputColor format=rgba16unorm filter=linear wrap=repeat shape=2d

// 2×1 strip; u = 1.375 → coord_x = 2.25 → floor 2 wraps to column 0, i1 wraps to column 1, frac = 0.25
// texture-data: inputColor 2x1 rgba16unorm
//   1.0,0.0,0.0,1.0  0.0,1.0,0.0,1.0

uniform sampler2D inputColor;

vec4 repeat_blend() {
    return texture(inputColor, vec2(1.375, 0.5));
}

// run: repeat_blend() ~= vec4(0.75, 0.25, 0.0, 1.0) (tolerance: 0.005)
