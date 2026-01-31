// test run
// target riscv32.q32

// ============================================================================
// lpfx_hue2rgb(): Convert hue value to RGB color
// ============================================================================

float test_lpfx_hue2rgb_red() {
    // Hue 0.0 should produce red (1, 0, 0)
    vec3 rgb = lpfx_hue2rgb(0.0);
    bool is_red = abs(rgb.r - 1.0) < 0.1 && rgb.g < 0.1 && rgb.b < 0.1;
    return is_red ? 1.0 : 0.0;
}

// run: test_lpfx_hue2rgb_red() == 1.0

float test_lpfx_hue2rgb_green() {
    // Hue ~0.333 should produce green (0, 1, 0)
    vec3 rgb = lpfx_hue2rgb(0.333);
    bool is_green = rgb.r < 0.1 && abs(rgb.g - 1.0) < 0.1 && rgb.b < 0.1;
    return is_green ? 1.0 : 0.0;
}

// run: test_lpfx_hue2rgb_green() == 1.0

float test_lpfx_hue2rgb_blue() {
    // Hue ~0.666 should produce blue (0, 0, 1)
    vec3 rgb = lpfx_hue2rgb(0.666);
    bool is_blue = rgb.r < 0.1 && rgb.g < 0.1 && abs(rgb.b - 1.0) < 0.1;
    return is_blue ? 1.0 : 0.0;
}

// run: test_lpfx_hue2rgb_blue() == 1.0

float test_lpfx_hue2rgb_range() {
    // All components should be in [0, 1] range
    vec3 rgb1 = lpfx_hue2rgb(0.0);
    vec3 rgb2 = lpfx_hue2rgb(0.5);
    vec3 rgb3 = lpfx_hue2rgb(1.0);
    
    bool valid = rgb1.r >= 0.0 && rgb1.r <= 1.0 &&
                 rgb1.g >= 0.0 && rgb1.g <= 1.0 &&
                 rgb1.b >= 0.0 && rgb1.b <= 1.0 &&
                 rgb2.r >= 0.0 && rgb2.r <= 1.0 &&
                 rgb2.g >= 0.0 && rgb2.g <= 1.0 &&
                 rgb2.b >= 0.0 && rgb2.b <= 1.0 &&
                 rgb3.r >= 0.0 && rgb3.r <= 1.0 &&
                 rgb3.g >= 0.0 && rgb3.g <= 1.0 &&
                 rgb3.b >= 0.0 && rgb3.b <= 1.0;
    return valid ? 1.0 : 0.0;
}

// run: test_lpfx_hue2rgb_range() == 1.0

float test_lpfx_hue2rgb_deterministic() {
    // Same hue should produce same RGB
    vec3 rgb1 = lpfx_hue2rgb(0.5);
    vec3 rgb2 = lpfx_hue2rgb(0.5);
    float diff = abs(rgb1.r - rgb2.r) + abs(rgb1.g - rgb2.g) + abs(rgb1.b - rgb2.b);
    return diff < 0.01 ? 1.0 : 0.0;
}

// run: test_lpfx_hue2rgb_deterministic() == 1.0
