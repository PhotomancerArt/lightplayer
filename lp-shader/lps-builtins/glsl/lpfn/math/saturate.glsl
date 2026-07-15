// lpfn_saturate — clamp to [0, 1] (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_saturate` builtins,
// matching `src/builtins/lpfn/math/saturate_q32.rs`.
//
// The saturate operation is standard mathematical procedure with no
// licensing concerns (see docs/reports/2026-03-31-lpfx-license-audit.md).

float lpfn_saturate(float x) {
    return clamp(x, 0.0, 1.0);
}

vec3 lpfn_saturate(vec3 v) {
    return clamp(v, vec3(0.0), vec3(1.0));
}

vec4 lpfn_saturate(vec4 v) {
    return clamp(v, vec4(0.0), vec4(1.0));
}
