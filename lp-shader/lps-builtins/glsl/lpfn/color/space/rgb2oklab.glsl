// lpfn_rgb2oklab — linear sRGB to Oklab (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_rgb2oklab` builtins,
// matching `src/builtins/lpfn/color/space/rgb2oklab_q32.rs`. The inverse of
// lpfn_oklab2rgb; see there for the space.
//
// GLSL has no cbrt: the signed cube root is spelled sign(x)*pow(|x|, 1/3), so
// out-of-gamut (negative) LMS values round-trip instead of producing NaN.

float lpfn_rgb2oklab_cbrt(float x) {
    return sign(x) * pow(abs(x), 1.0 / 3.0);
}

vec3 lpfn_rgb2oklab(vec3 rgb) {
    float l = 0.4122214708 * rgb.x + 0.5363325363 * rgb.y + 0.0514459929 * rgb.z;
    float m = 0.2119034982 * rgb.x + 0.6806995451 * rgb.y + 0.1073969566 * rgb.z;
    float s = 0.0883024619 * rgb.x + 0.2817188376 * rgb.y + 0.6299787005 * rgb.z;
    float l_ = lpfn_rgb2oklab_cbrt(l);
    float m_ = lpfn_rgb2oklab_cbrt(m);
    float s_ = lpfn_rgb2oklab_cbrt(s);
    return vec3(
        0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
        1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
        0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_
    );
}
