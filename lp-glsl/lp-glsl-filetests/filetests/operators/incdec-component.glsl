// test run
// target riscv32.q32

// ============================================================================
// Pre-increment (++v.x) - Single component
// ============================================================================

float test_preinc_component_x() {
    vec2 v = vec2(1.0, 2.0);
    float result = ++v.x;  // v.x becomes 2.0, result is 2.0
    return result + v.x + v.y;  // Should be 2.0 + 2.0 + 2.0 = 6.0
}

// run: test_preinc_component_x() ~= 6.0

float test_preinc_component_y() {
    vec2 v = vec2(1.0, 2.0);
    float result = ++v.y;  // v.y becomes 3.0, result is 3.0
    return result + v.x + v.y;  // Should be 3.0 + 1.0 + 3.0 = 7.0
}

// run: test_preinc_component_y() ~= 7.0

float test_preinc_component_z() {
    vec3 v = vec3(1.0, 2.0, 3.0);
    float result = ++v.z;  // v.z becomes 4.0, result is 4.0
    return result + v.x + v.y + v.z;  // Should be 4.0 + 1.0 + 2.0 + 4.0 = 11.0
}

// run: test_preinc_component_z() ~= 11.0

float test_preinc_component_w() {
    vec4 v = vec4(1.0, 2.0, 3.0, 4.0);
    float result = ++v.w;  // v.w becomes 5.0, result is 5.0
    return result + v.x + v.y + v.z + v.w;  // Should be 5.0 + 1.0 + 2.0 + 3.0 + 5.0 = 16.0
}

// run: test_preinc_component_w() ~= 16.0

int test_preinc_component_int() {
    ivec2 v = ivec2(5, 10);
    int result = ++v.x;  // v.x becomes 6, result is 6
    return result + v.x + v.y;  // Should be 6 + 6 + 10 = 22
}

// run: test_preinc_component_int() == 22

// ============================================================================
// Post-increment (v.x++) - Single component
// ============================================================================

float test_postinc_component_x() {
    vec2 v = vec2(1.0, 2.0);
    float old_x = v.x++;
    return old_x + v.x;  // Should be 1.0 + 2.0 = 3.0
}

// run: test_postinc_component_x() ~= 3.0

float test_postinc_component_y() {
    vec2 v = vec2(1.0, 2.0);
    float old_y = v.y++;
    return old_y + v.y;  // Should be 2.0 + 3.0 = 5.0
}

// run: test_postinc_component_y() ~= 5.0

float test_postinc_component_z() {
    vec3 v = vec3(1.0, 2.0, 3.0);
    float old_z = v.z++;
    return old_z + v.z;  // Should be 3.0 + 4.0 = 7.0
}

// run: test_postinc_component_z() ~= 7.0

float test_postinc_component_w() {
    vec4 v = vec4(1.0, 2.0, 3.0, 4.0);
    float old_w = v.w++;
    return old_w + v.w;  // Should be 4.0 + 5.0 = 9.0
}

// run: test_postinc_component_w() ~= 9.0

int test_postinc_component_int() {
    ivec2 v = ivec2(5, 10);
    int old_x = v.x++;
    return old_x + v.x;  // Should be 5 + 6 = 11
}

// run: test_postinc_component_int() == 11

// ============================================================================
// Pre-decrement (--v.x) - Single component
// ============================================================================

float test_predec_component_x() {
    vec2 v = vec2(3.0, 4.0);
    float result = --v.x;  // v.x becomes 2.0, result is 2.0
    return result + v.x + v.y;  // Should be 2.0 + 2.0 + 4.0 = 8.0
}

// run: test_predec_component_x() ~= 8.0

float test_predec_component_y() {
    vec2 v = vec2(3.0, 4.0);
    float result = --v.y;  // v.y becomes 3.0, result is 3.0
    return result + v.x + v.y;  // Should be 3.0 + 3.0 + 3.0 = 9.0
}

// run: test_predec_component_y() ~= 9.0

float test_predec_component_z() {
    vec3 v = vec3(3.0, 4.0, 5.0);
    float result = --v.z;  // v.z becomes 4.0, result is 4.0
    return result + v.x + v.y + v.z;  // Should be 4.0 + 3.0 + 4.0 + 4.0 = 15.0
}

// run: test_predec_component_z() ~= 15.0

float test_predec_component_w() {
    vec4 v = vec4(3.0, 4.0, 5.0, 6.0);
    float result = --v.w;  // v.w becomes 5.0, result is 5.0
    return result + v.x + v.y + v.z + v.w;  // Should be 5.0 + 3.0 + 4.0 + 5.0 + 5.0 = 22.0
}

// run: test_predec_component_w() ~= 22.0

int test_predec_component_int() {
    ivec2 v = ivec2(5, 10);
    int result = --v.x;  // v.x becomes 4, result is 4
    return result + v.x + v.y;  // Should be 4 + 4 + 10 = 18
}

// run: test_predec_component_int() == 18

// ============================================================================
// Post-decrement (v.x--) - Single component
// ============================================================================

float test_postdec_component_x() {
    vec2 v = vec2(3.0, 4.0);
    float old_x = v.x--;
    return old_x + v.x;  // Should be 3.0 + 2.0 = 5.0
}

// run: test_postdec_component_x() ~= 5.0

float test_postdec_component_y() {
    vec2 v = vec2(3.0, 4.0);
    float old_y = v.y--;
    return old_y + v.y;  // Should be 4.0 + 3.0 = 7.0
}

// run: test_postdec_component_y() ~= 7.0

float test_postdec_component_z() {
    vec3 v = vec3(3.0, 4.0, 5.0);
    float old_z = v.z--;
    return old_z + v.z;  // Should be 5.0 + 4.0 = 9.0
}

// run: test_postdec_component_z() ~= 9.0

float test_postdec_component_w() {
    vec4 v = vec4(3.0, 4.0, 5.0, 6.0);
    float old_w = v.w--;
    return old_w + v.w;  // Should be 6.0 + 5.0 = 11.0
}

// run: test_postdec_component_w() ~= 11.0

int test_postdec_component_int() {
    ivec2 v = ivec2(5, 10);
    int old_x = v.x--;
    return old_x + v.x;  // Should be 5 + 4 = 9
}

// run: test_postdec_component_int() == 9

// ============================================================================
// Multi-component access (v.xy++, etc.) - Note: These may not be valid LValues
// ============================================================================

// Note: Multi-component swizzles like v.xy++ may not be valid LValues in GLSL.
// If they are supported, tests would go here. For now, we focus on single components.






