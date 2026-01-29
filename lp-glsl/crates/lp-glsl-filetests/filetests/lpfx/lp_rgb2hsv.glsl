// test run
// target riscv32.q32

// ============================================================================
// lpfx_rgb2hsv(): Convert RGB color space to HSV
// ============================================================================

float test_lpfx_rgb2hsv_pure_red() {
    // RGB(1, 0, 0) -> HSV(0, 1, 1) approximately
    vec3 rgb = vec3(1.0, 0.0, 0.0);
    vec3 hsv = lpfx_rgb2hsv(rgb);
    bool valid = (hsv.x < 0.01 || abs(hsv.x - 1.0) < 0.01) &&
                 abs(hsv.y - 1.0) < 0.1 &&
                 abs(hsv.z - 1.0) < 0.1;
    return valid ? 1.0 : 0.0;
}

// run: test_lpfx_rgb2hsv_pure_red() == 1.0

float test_lpfx_rgb2hsv_black() {
    // RGB(0, 0, 0) -> HSV(0, 0, 0)
    vec3 rgb = vec3(0.0, 0.0, 0.0);
    vec3 hsv = lpfx_rgb2hsv(rgb);
    bool is_black = hsv.x < 0.01 && hsv.y < 0.01 && hsv.z < 0.01;
    return is_black ? 1.0 : 0.0;
}

// run: test_lpfx_rgb2hsv_black() == 1.0

float test_lpfx_rgb2hsv_white() {
    // RGB(1, 1, 1) -> HSV(0, 0, 1)
    vec3 rgb = vec3(1.0, 1.0, 1.0);
    vec3 hsv = lpfx_rgb2hsv(rgb);
    bool is_white = hsv.y < 0.01 && abs(hsv.z - 1.0) < 0.01;
    return is_white ? 1.0 : 0.0;
}

// run: test_lpfx_rgb2hsv_white() == 1.0

float test_lpfx_rgb2hsv_grayscale() {
    // Grayscale colors should have saturation = 0
    vec3 gray1 = vec3(0.5, 0.5, 0.5);
    vec3 gray2 = vec3(0.3, 0.3, 0.3);
    vec3 hsv1 = lpfx_rgb2hsv(gray1);
    vec3 hsv2 = lpfx_rgb2hsv(gray2);
    bool valid = hsv1.y < 0.01 && hsv2.y < 0.01;
    return valid ? 1.0 : 0.0;
}

// run: test_lpfx_rgb2hsv_grayscale() == 1.0

float test_lpfx_rgb2hsv_vec4() {
    // Test vec4 version preserves alpha
    vec4 rgb = vec4(1.0, 0.0, 0.0, 0.7);
    vec4 hsv = lpfx_rgb2hsv(rgb);
    bool valid = abs(hsv.a - 0.7) < 0.01;
    return valid ? 1.0 : 0.0;
}

// run: test_lpfx_rgb2hsv_vec4() == 1.0

float test_lpfx_rgb2hsv_range() {
    // HSV components should be in [0, 1] range
    vec3 rgb1 = vec3(1.0, 0.0, 0.0);
    vec3 rgb2 = vec3(0.5, 0.3, 0.8);
    vec3 hsv1 = lpfx_rgb2hsv(rgb1);
    vec3 hsv2 = lpfx_rgb2hsv(rgb2);
    
    bool valid = hsv1.x >= 0.0 && hsv1.x <= 1.0 &&
                 hsv1.y >= 0.0 && hsv1.y <= 1.0 &&
                 hsv1.z >= 0.0 && hsv1.z <= 1.0 &&
                 hsv2.x >= 0.0 && hsv2.x <= 1.0 &&
                 hsv2.y >= 0.0 && hsv2.y <= 1.0 &&
                 hsv2.z >= 0.0 && hsv2.z <= 1.0;
    return valid ? 1.0 : 0.0;
}

// run: test_lpfx_rgb2hsv_range() == 1.0
