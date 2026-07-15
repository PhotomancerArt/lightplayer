// lpfn_rgb2hsv — RGB to HSV conversion (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_rgb2hsv` builtins,
// matching `src/builtins/lpfn/color/space/rgb2hsv_q32.rs`.
//
// Algorithm: Sam Hocevar's branch-minimizing RGB→HSV
// (http://lolengine.net/blog/2013/07/27/rgb-to-hsv-in-glsl), a widely
// referenced standard formulation (see
// docs/reports/2026-03-31-lpfx-license-audit.md).
//
// NOTE: the epsilon is 1/65536 — the smallest positive Q16.16 value — chosen
// so the canonical float semantics and the Q32 device implementation use the
// same guard against division by zero (LightPlayer semantics, deliberate
// deviation from LYGIA's 1e-10).

vec3 lpfn_rgb2hsv(vec3 rgb) {
    float epsilon = 1.0 / 65536.0;
    vec3 c = rgb;
    vec4 p = (c.y < c.z) ? vec4(c.z, c.y, -1.0, 2.0 / 3.0)
                         : vec4(c.y, c.z, 0.0, -1.0 / 3.0);
    vec4 q = (c.x < p.x) ? vec4(p.x, p.y, p.w, c.x)
                         : vec4(c.x, p.y, p.z, p.x);
    float d = q.x - min(q.w, q.y);
    float h = abs(q.z + (q.w - q.y) / (6.0 * d + epsilon));
    float s = d / (q.x + epsilon);
    float v = q.x;
    return vec3(h, s, v);
}

vec4 lpfn_rgb2hsv(vec4 rgb) {
    return vec4(lpfn_rgb2hsv(rgb.xyz), rgb.w);
}
