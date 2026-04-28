// test run

// Nearest + clamp-to-edge; sample with u,v outside [0,1] → edge texels.

// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=clamp shape=2d

// 2×1 strip: left (red) and right (green)
// texture-data: inputColor 2x1 rgba16unorm
//   1.0,0.0,0.0,1.0  0.0,1.0,0.0,1.0

uniform sampler2D inputColor;

vec4 sample_left_edge() {
    return texture(inputColor, vec2(-0.2, 0.5));
}

vec4 sample_right_edge() {
    return texture(inputColor, vec2(1.35, 0.5));
}

// u<0 → column 0; u>1 → column 1
// run: sample_left_edge() ~= vec4(1.0, 0.0, 0.0, 1.0) (tolerance: 0.0003)
// run: sample_right_edge() ~= vec4(0.0, 1.0, 0.0, 1.0) (tolerance: 0.0003)
