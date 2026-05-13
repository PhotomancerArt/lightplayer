// test run

// Nearest + mirror-repeat on X (builtin mirror index mapping).

// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=mirror-repeat shape=2d

// 3×1: distinct RGBA per column; choose u so the wrapped index lands on column 1.
// texture-data: inputColor 3x1 rgba16unorm
//   1.0,0.0,0.0,1.0  0.0,1.0,0.0,1.0  0.0,0.0,1.0,1.0

uniform sampler2D inputColor;

vec4 mirror_sample() {
    return texture(inputColor, vec2(1.15, 0.5));
}

// coord_x = 1.15*3 - 0.5 = 2.95 → nearest 3 → mirror maps to column 1 (green)
// run: mirror_sample() ~= vec4(0.0, 1.0, 0.0, 1.0) (tolerance: 0.0004)
