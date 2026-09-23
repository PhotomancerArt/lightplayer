// lpfn_rgb2oklch — linear sRGB to Oklch (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_rgb2oklch` builtins,
// matching `src/builtins/lpfn/color/space/rgb2oklch_q32.rs`.
//
// Hue is in TURNS, wrapped to [0, 1). A grey has no hue; its h is whatever
// atan2 makes of two roundoff values, so read C before trusting h.
//
// Depends on: color/space/rgb2oklab.glsl

vec3 lpfn_rgb2oklch(vec3 rgb) {
    vec3 lab = lpfn_rgb2oklab(rgb);
    float c = sqrt(lab.y * lab.y + lab.z * lab.z);
    float h = fract(atan(lab.z, lab.y) / 6.28318530718);
    return vec3(lab.x, c, h);
}
