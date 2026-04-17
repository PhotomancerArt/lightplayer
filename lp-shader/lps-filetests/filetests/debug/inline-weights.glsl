// Debug corpus for M3.1 inline `func_weight` tuning (`lp-cli shader-debug --weights`).
// Many small helpers + entry points; no `// run:` expectations (validate-only).

float iw_lerp(float a, float b, float t) {
    return mix(a, b, t);
}

float iw_clamp01(float x) {
    return clamp(x, 0.0, 1.0);
}

vec3 iw_mul3(vec3 v, float s) {
    return v * s;
}

vec3 iw_add3(vec3 a, vec3 b) {
    return a + b;
}

vec3 iw_palette_dispatch(float t, float k) {
    if (k < 0.5) {
        return mix(vec3(0.0), vec3(1.0), t);
    }
    if (k < 1.5) {
        return iw_add3(vec3(t), vec3(0.1));
    }
    if (k < 2.5) {
        return iw_mul3(vec3(1.0 - t), 0.5);
    }
    if (k < 3.5) {
        return vec3(iw_clamp01(t * 2.0));
    }
    return vec3(sqrt(iw_clamp01(t)));
}

float iw_step01(float x, float edge) {
    if (x < edge) {
        return 0.0;
    }
    return 1.0;
}

vec3 iw_builtin_stack(float u, float v) {
    float a = sqrt(clamp(u, 0.0, 1.0));
    float b = cos(v * 3.14159265);
    float c = mix(a, b, 0.37);
    float d = sqrt(clamp(mix(u, v, c), 0.0, 1.0));
    float e = cos(d * 2.0);
    return vec3(mix(c, e, 0.2), sqrt(abs(b)), clamp(a * d, 0.0, 1.0));
}

float iw_vec3_len_custom(vec3 v) {
    float s = v.x * v.x + v.y * v.y + v.z * v.z;
    return sqrt(s);
}

vec3 iw_color_grade(vec3 rgb, float exposure, float lift, float sat) {
    vec3 lifted = rgb * exposure + vec3(lift);
    float luma = dot(lifted, vec3(0.299, 0.587, 0.114));
    vec3 chroma = lifted - vec3(luma);
    vec3 adj = vec3(luma) + chroma * sat;
    return clamp(mix(lifted, adj, 0.65), vec3(0.0), vec3(1.0));
}

vec3 iw_noise_blend(vec3 p, float blend, float mode) {
    vec3 a = iw_builtin_stack(p.x, p.y);
    vec3 b = iw_color_grade(a, 1.1, 0.02, 1.05);
    vec3 c = iw_palette_dispatch(blend, mode);
    vec3 d = iw_mul3(iw_add3(b, c), 0.5);
    float len = iw_vec3_len_custom(d + vec3(0.01));
    vec3 e = iw_builtin_stack(len, p.z);
    float edge = iw_step01(blend, 0.33);
    vec3 f = mix(d, e, edge);
    return clamp(f, vec3(0.0), vec3(1.0));
}

float iw_twist(float x, float amt) {
    float y = fract(x + amt);
    return iw_lerp(x, y, 0.5);
}

vec3 iw_fold_rgb(vec3 v) {
    return abs(v * 2.0 - vec3(1.0));
}

vec3 test_inline_weights_entry_a() {
    return iw_noise_blend(vec3(0.2, 0.7, 0.3), 0.4, 1.0);
}

vec3 test_inline_weights_entry_b() {
    vec3 p = iw_palette_dispatch(0.5, 2.0);
    vec3 q = iw_builtin_stack(0.25, 0.5);
    float t = iw_twist(0.3, 0.11);
    return iw_add3(iw_mul3(p, 0.9), iw_mul3(q, 0.1 + t * 0.02));
}

vec3 test_inline_weights_entry_c() {
    return iw_fold_rgb(iw_color_grade(vec3(0.4, 0.5, 0.6), 0.95, 0.03, 1.2));
}
