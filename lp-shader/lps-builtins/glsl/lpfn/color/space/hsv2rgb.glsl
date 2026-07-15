// lpfn_hsv2rgb — HSV to RGB conversion (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_hsv2rgb` builtins,
// matching `src/builtins/lpfn/color/space/hsv2rgb_q32.rs`.
//
// HSV→RGB conversion is standard mathematical procedure (Foley & van Dam);
// the LightPlayer port was originally written with reference to LYGIA's
// hsv2rgb.glsl (see docs/reports/2026-03-31-lpfx-license-audit.md).
//
// Depends on: color/space/hue2rgb.glsl (which depends on math/saturate.glsl)

vec3 lpfn_hsv2rgb(vec3 hsv) {
    // ((hue2rgb(h) - 1.0) * s + 1.0) * v
    vec3 rgb = lpfn_hue2rgb(hsv.x);
    return ((rgb - 1.0) * hsv.y + 1.0) * hsv.z;
}

vec4 lpfn_hsv2rgb(vec4 hsv) {
    return vec4(lpfn_hsv2rgb(hsv.xyz), hsv.w);
}
