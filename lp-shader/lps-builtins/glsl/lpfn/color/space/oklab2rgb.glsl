// lpfn_oklab2rgb — Oklab to linear sRGB (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_oklab2rgb` builtins,
// matching `src/builtins/lpfn/color/space/oklab2rgb_q32.rs`.
//
// Oklab is Björn Ottosson's perceptual space
// (https://bottosson.github.io/posts/oklab/); the matrices are its published
// definition. "rgb" is linear sRGB, LightPlayer's canonical color space, and
// the result is not clamped.

vec3 lpfn_oklab2rgb(vec3 lab) {
    float l_ = lab.x + 0.3963377774 * lab.y + 0.2158037573 * lab.z;
    float m_ = lab.x - 0.1055613458 * lab.y - 0.0638541728 * lab.z;
    float s_ = lab.x - 0.0894841775 * lab.y - 1.2914855480 * lab.z;
    float l3 = l_ * l_ * l_;
    float m3 = m_ * m_ * m_;
    float s3 = s_ * s_ * s_;
    return vec3(
         4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3,
        -1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3,
        -0.0041960863 * l3 - 0.7034186147 * m3 + 1.7076147010 * s3
    );
}
