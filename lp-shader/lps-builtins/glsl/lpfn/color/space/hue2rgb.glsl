// lpfn_hue2rgb — hue value to RGB color (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_hue2rgb` builtin,
// matching `src/builtins/lpfn/color/space/hue2rgb_q32.rs`.
//
// The hue2rgb formula (abs/arithmetic ramp per channel) is standard color
// space mathematics documented in graphics literature; the LightPlayer port
// was originally written with reference to LYGIA's hue2rgb.glsl
// (see docs/reports/2026-03-31-lpfx-license-audit.md).
//
// Depends on: math/saturate.glsl

vec3 lpfn_hue2rgb(float hue) {
    float h6 = hue * 6.0;
    float r = abs(h6 - 3.0) - 1.0;
    float g = 2.0 - abs(h6 - 2.0);
    float b = 2.0 - abs(h6 - 4.0);
    return lpfn_saturate(vec3(r, g, b));
}
