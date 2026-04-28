// test run

// Linear + clamp; bilinear blend (not exactly one texel).

// texture-spec: inputColor format=rgba16unorm filter=linear wrap=clamp shape=2d

// texture-data: inputColor 2x1 rgba16unorm
//   1.0,0.0,0.0,1.0  0.0,1.0,0.0,1.0

uniform sampler2D inputColor;

vec4 center_blend() {
    return texture(inputColor, vec2(0.5, 0.5));
}

// u=0.5 → halfway between columns → R and G both ~0.5
// run: center_blend() ~= vec4(0.5, 0.5, 0.0, 1.0) (tolerance: 0.004)
