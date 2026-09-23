// test run

// ============================================================================
// lpfn_oklab2rgb / lpfn_rgb2oklab / lpfn_oklch2rgb / lpfn_rgb2oklch:
// Oklab and its polar form, to and from linear RGB. Oklch hue is in turns.
// ============================================================================

float test_lpfn_oklab2rgb_white() {
    vec3 rgb = lpfn_oklab2rgb(vec3(1.0, 0.0, 0.0));
    bool ok = abs(rgb.x - 1.0) < 0.01 && abs(rgb.y - 1.0) < 0.01 && abs(rgb.z - 1.0) < 0.01;
    return ok ? 1.0 : 0.0;
}

// run: test_lpfn_oklab2rgb_white() == 1.0

float test_lpfn_rgb2oklab_red() {
    // Published: linear sRGB red is Oklab (0.628, 0.225, 0.126).
    vec3 lab = lpfn_rgb2oklab(vec3(1.0, 0.0, 0.0));
    bool ok = abs(lab.x - 0.628) < 0.01 && abs(lab.y - 0.225) < 0.01 && abs(lab.z - 0.126) < 0.01;
    return ok ? 1.0 : 0.0;
}

// run: test_lpfn_rgb2oklab_red() == 1.0

float test_lpfn_oklab_round_trip() {
    vec3 rgb = vec3(0.75, 0.25, 0.5);
    vec3 back = lpfn_oklab2rgb(lpfn_rgb2oklab(rgb));
    return length(back - rgb) < 0.01 ? 1.0 : 0.0;
}

// run: test_lpfn_oklab_round_trip() == 1.0

float test_lpfn_oklch_round_trip() {
    vec3 rgb = vec3(0.1, 0.9, 0.4);
    vec3 back = lpfn_oklch2rgb(lpfn_rgb2oklch(rgb));
    return length(back - rgb) < 0.02 ? 1.0 : 0.0;
}

// run: test_lpfn_oklch_round_trip() == 1.0

float test_lpfn_oklch_hue_is_turns() {
    // Red's Oklch hue is 29.2 degrees = 0.081 turns.
    vec3 lch = lpfn_rgb2oklch(vec3(1.0, 0.0, 0.0));
    return abs(lch.z - 0.081) < 0.01 ? 1.0 : 0.0;
}

// run: test_lpfn_oklch_hue_is_turns() == 1.0

float test_lpfn_oklch_hue_wraps() {
    vec3 a = lpfn_oklch2rgb(vec3(0.75, 0.125, 0.25));
    vec3 b = lpfn_oklch2rgb(vec3(0.75, 0.125, -0.75));
    vec3 c = lpfn_oklch2rgb(vec3(0.75, 0.125, 2.25));
    return (length(a - b) < 0.001 && length(a - c) < 0.001) ? 1.0 : 0.0;
}

// run: test_lpfn_oklch_hue_wraps() == 1.0

float test_lpfn_oklch_constant_lightness() {
    // Holding L and C, every hue lands at the same Oklab L.
    float lo = 1.0;
    float hi = 0.0;
    for (int i = 0; i < 12; i++) {
        vec3 rgb = lpfn_oklch2rgb(vec3(0.75, 0.125, float(i) / 12.0));
        float l = lpfn_rgb2oklab(rgb).x;
        lo = min(lo, l);
        hi = max(hi, l);
    }
    return (hi - lo) < 0.01 ? 1.0 : 0.0;
}

// run: test_lpfn_oklch_constant_lightness() == 1.0
