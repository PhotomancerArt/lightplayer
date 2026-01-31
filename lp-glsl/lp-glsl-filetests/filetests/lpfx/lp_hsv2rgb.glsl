// test run
// target riscv32.q32

// ============================================================================
// lpfx_hsv2rgb(): Convert HSV color space to RGB
// ============================================================================

float test_lpfx_hsv2rgb_pure_red() {
    // HSV(0, 1, 1) -> RGB(1, 0, 0)
    vec3 hsv = vec3(0.0, 1.0, 1.0);
    vec3 rgb = lpfx_hsv2rgb(hsv);
    bool is_red = abs(rgb.r - 1.0) < 0.1 && rgb.g < 0.1 && rgb.b < 0.1;
    return is_red ? 1.0 : 0.0;
}

// run: test_lpfx_hsv2rgb_pure_red() == 1.0

float test_lpfx_hsv2rgb_black() {
    // HSV(0, 0, 0) -> RGB(0, 0, 0)
    vec3 hsv = vec3(0.0, 0.0, 0.0);
    vec3 rgb = lpfx_hsv2rgb(hsv);
    bool is_black = rgb.r < 0.01 && rgb.g < 0.01 && rgb.b < 0.01;
    return is_black ? 1.0 : 0.0;
}

// run: test_lpfx_hsv2rgb_black() == 1.0

float test_lpfx_hsv2rgb_white() {
    // HSV(0, 0, 1) -> RGB(1, 1, 1)
    vec3 hsv = vec3(0.0, 0.0, 1.0);
    vec3 rgb = lpfx_hsv2rgb(hsv);
    bool is_white = abs(rgb.r - 1.0) < 0.01 && 
                    abs(rgb.g - 1.0) < 0.01 && 
                    abs(rgb.b - 1.0) < 0.01;
    return is_white ? 1.0 : 0.0;
}

// run: test_lpfx_hsv2rgb_white() == 1.0

float test_lpfx_hsv2rgb_vec4() {
    // Test vec4 version preserves alpha
    vec4 hsv = vec4(0.0, 1.0, 1.0, 0.5);
    vec4 rgb = lpfx_hsv2rgb(hsv);
    bool valid = abs(rgb.r - 1.0) < 0.1 && 
                  rgb.g < 0.1 && 
                  rgb.b < 0.1 &&
                  abs(rgb.a - 0.5) < 0.01;
    return valid ? 1.0 : 0.0;
}

// run: test_lpfx_hsv2rgb_vec4() == 1.0

float test_lpfx_hsv2rgb_range() {
    // RGB components should be in [0, 1] range
    vec3 hsv1 = vec3(0.0, 1.0, 1.0);
    vec3 hsv2 = vec3(0.5, 0.7, 0.8);
    vec3 rgb1 = lpfx_hsv2rgb(hsv1);
    vec3 rgb2 = lpfx_hsv2rgb(hsv2);
    
    bool valid = rgb1.r >= 0.0 && rgb1.r <= 1.0 &&
                 rgb1.g >= 0.0 && rgb1.g <= 1.0 &&
                 rgb1.b >= 0.0 && rgb1.b <= 1.0 &&
                 rgb2.r >= 0.0 && rgb2.r <= 1.0 &&
                 rgb2.g >= 0.0 && rgb2.g <= 1.0 &&
                 rgb2.b >= 0.0 && rgb2.b <= 1.0;
    return valid ? 1.0 : 0.0;
}

// run: test_lpfx_hsv2rgb_range() == 1.0
