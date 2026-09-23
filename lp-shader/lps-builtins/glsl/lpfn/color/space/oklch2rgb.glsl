// lpfn_oklch2rgb — Oklch to linear sRGB (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_oklch2rgb` builtins,
// matching `src/builtins/lpfn/color/space/oklch2rgb_q32.rs`.
//
// Oklch is Oklab in polar form. The hue is in TURNS (0..1, wrapping), like
// lpfn_hsv2rgb's, not degrees.
//
// Depends on: color/space/oklab2rgb.glsl

vec3 lpfn_oklch2rgb(vec3 lch) {
    float angle = fract(lch.z) * 6.28318530718;
    return lpfn_oklab2rgb(vec3(lch.x, lch.y * cos(angle), lch.y * sin(angle)));
}
