// test run
// target riscv32.q32

// ============================================================================
// Pre-increment (++v) - vec2
// ============================================================================

float test_preinc_vec2() {
    vec2 v = vec2(1.0, 2.0);
    vec2 result = ++v;  // Should increment v to (2.0, 3.0), then return (2.0, 3.0)
    return result.x + result.y;  // Should be 2.0 + 3.0 = 5.0
}

// run: test_preinc_vec2() ~= 5.0

// ============================================================================
// Post-increment (v++) - vec2
// ============================================================================

float test_postinc_vec2() {
    vec2 v = vec2(1.0, 2.0);
    vec2 old_v = v++;
    return old_v.x + old_v.y;  // Should be 1.0 + 2.0 = 3.0
}

// run: test_postinc_vec2() ~= 3.0

// ============================================================================
// Pre-decrement (--v) - vec2
// ============================================================================

float test_predec_vec2() {
    vec2 v = vec2(3.0, 4.0);
    vec2 result = --v;  // Should decrement v to (2.0, 3.0), then return (2.0, 3.0)
    return result.x + result.y;  // Should be 2.0 + 3.0 = 5.0
}

// run: test_predec_vec2() ~= 5.0

// ============================================================================
// Post-decrement (v--) - vec2
// ============================================================================

float test_postdec_vec2() {
    vec2 v = vec2(3.0, 4.0);
    vec2 old_v = v--;
    return old_v.x + old_v.y;  // Should be 3.0 + 4.0 = 7.0
}

// run: test_postdec_vec2() ~= 7.0

// ============================================================================
// Pre-increment (++v) - vec3
// ============================================================================

float test_preinc_vec3() {
    vec3 v = vec3(1.0, 2.0, 3.0);
    vec3 result = ++v;  // Should increment v to (2.0, 3.0, 4.0), then return (2.0, 3.0, 4.0)
    return result.x + result.y + result.z;  // Should be 2.0 + 3.0 + 4.0 = 9.0
}

// run: test_preinc_vec3() ~= 9.0

// ============================================================================
// Post-increment (v++) - vec3
// ============================================================================

float test_postinc_vec3() {
    vec3 v = vec3(1.0, 2.0, 3.0);
    vec3 old_v = v++;
    return old_v.x + old_v.y + old_v.z;  // Should be 1.0 + 2.0 + 3.0 = 6.0
}

// run: test_postinc_vec3() ~= 6.0

// ============================================================================
// Pre-decrement (--v) - vec3
// ============================================================================

float test_predec_vec3() {
    vec3 v = vec3(3.0, 4.0, 5.0);
    vec3 result = --v;  // Should decrement v to (2.0, 3.0, 4.0), then return (2.0, 3.0, 4.0)
    return result.x + result.y + result.z;  // Should be 2.0 + 3.0 + 4.0 = 9.0
}

// run: test_predec_vec3() ~= 9.0

// ============================================================================
// Post-decrement (v--) - vec3
// ============================================================================

float test_postdec_vec3() {
    vec3 v = vec3(3.0, 4.0, 5.0);
    vec3 old_v = v--;
    return old_v.x + old_v.y + old_v.z;  // Should be 3.0 + 4.0 + 5.0 = 12.0
}

// run: test_postdec_vec3() ~= 12.0

// ============================================================================
// Pre-increment (++v) - vec4
// ============================================================================

float test_preinc_vec4() {
    vec4 v = vec4(1.0, 2.0, 3.0, 4.0);
    vec4 result = ++v;  // Should increment v to (2.0, 3.0, 4.0, 5.0), then return (2.0, 3.0, 4.0, 5.0)
    return result.x + result.y + result.z + result.w;  // Should be 2.0 + 3.0 + 4.0 + 5.0 = 14.0
}

// run: test_preinc_vec4() ~= 14.0

// ============================================================================
// Post-increment (v++) - vec4
// ============================================================================

float test_postinc_vec4() {
    vec4 v = vec4(1.0, 2.0, 3.0, 4.0);
    vec4 old_v = v++;
    return old_v.x + old_v.y + old_v.z + old_v.w;  // Should be 1.0 + 2.0 + 3.0 + 4.0 = 10.0
}

// run: test_postinc_vec4() ~= 10.0

// ============================================================================
// Pre-decrement (--v) - vec4
// ============================================================================

float test_predec_vec4() {
    vec4 v = vec4(3.0, 4.0, 5.0, 6.0);
    vec4 result = --v;  // Should decrement v to (2.0, 3.0, 4.0, 5.0), then return (2.0, 3.0, 4.0, 5.0)
    return result.x + result.y + result.z + result.w;  // Should be 2.0 + 3.0 + 4.0 + 5.0 = 14.0
}

// run: test_predec_vec4() ~= 14.0

// ============================================================================
// Post-decrement (v--) - vec4
// ============================================================================

float test_postdec_vec4() {
    vec4 v = vec4(3.0, 4.0, 5.0, 6.0);
    vec4 old_v = v--;
    return old_v.x + old_v.y + old_v.z + old_v.w;  // Should be 3.0 + 4.0 + 5.0 + 6.0 = 18.0
}

// run: test_postdec_vec4() ~= 18.0

// ============================================================================
// Pre-increment (++v) - ivec2
// ============================================================================

int test_preinc_ivec2() {
    ivec2 v = ivec2(1, 2);
    ivec2 result = ++v;  // Should increment v to (2, 3), then return (2, 3)
    return result.x + result.y;  // Should be 2 + 3 = 5
}

// run: test_preinc_ivec2() == 5

// ============================================================================
// Post-increment (v++) - ivec2
// ============================================================================

int test_postinc_ivec2() {
    ivec2 v = ivec2(1, 2);
    ivec2 old_v = v++;
    return old_v.x + old_v.y;  // Should be 1 + 2 = 3
}

// run: test_postinc_ivec2() == 3

// ============================================================================
// Pre-decrement (--v) - ivec2
// ============================================================================

int test_predec_ivec2() {
    ivec2 v = ivec2(3, 4);
    ivec2 result = --v;  // Should decrement v to (2, 3), then return (2, 3)
    return result.x + result.y;  // Should be 2 + 3 = 5
}

// run: test_predec_ivec2() == 5

// ============================================================================
// Post-decrement (v--) - ivec2
// ============================================================================

int test_postdec_ivec2() {
    ivec2 v = ivec2(3, 4);
    ivec2 old_v = v--;
    return old_v.x + old_v.y;  // Should be 3 + 4 = 7
}

// run: test_postdec_ivec2() == 7

// ============================================================================
// Pre-increment (++v) - ivec3
// ============================================================================

int test_preinc_ivec3() {
    ivec3 v = ivec3(5, 10, 15);
    ivec3 result = ++v;  // Should increment v to (6, 11, 16), then return (6, 11, 16)
    return result.x + result.y + result.z;  // Should be 6 + 11 + 16 = 33
}

// run: test_preinc_ivec3() == 33

// ============================================================================
// Post-increment (v++) - ivec3
// ============================================================================

int test_postinc_ivec3() {
    ivec3 v = ivec3(5, 10, 15);
    ivec3 old_v = v++;
    return old_v.x + old_v.y + old_v.z;  // Should be 5 + 10 + 15 = 30
}

// run: test_postinc_ivec3() == 30

// ============================================================================
// Pre-decrement (--v) - ivec3
// ============================================================================

int test_predec_ivec3() {
    ivec3 v = ivec3(5, 10, 15);
    ivec3 result = --v;  // Should decrement v to (4, 9, 14), then return (4, 9, 14)
    return result.x + result.y + result.z;  // Should be 4 + 9 + 14 = 27
}

// run: test_predec_ivec3() == 27

// ============================================================================
// Post-decrement (v--) - ivec3
// ============================================================================

int test_postdec_ivec3() {
    ivec3 v = ivec3(5, 10, 15);
    ivec3 old_v = v--;
    return old_v.x + old_v.y + old_v.z;  // Should be 5 + 10 + 15 = 30
}

// run: test_postdec_ivec3() == 30

// ============================================================================
// Pre-increment (++v) - ivec4
// ============================================================================

int test_preinc_ivec4() {
    ivec4 v = ivec4(1, 2, 3, 4);
    ivec4 result = ++v;  // Should increment v to (2, 3, 4, 5), then return (2, 3, 4, 5)
    return result.x + result.y + result.z + result.w;  // Should be 2 + 3 + 4 + 5 = 14
}

// run: test_preinc_ivec4() == 14

// ============================================================================
// Post-increment (v++) - ivec4
// ============================================================================

int test_postinc_ivec4() {
    ivec4 v = ivec4(1, 2, 3, 4);
    ivec4 old_v = v++;
    return old_v.x + old_v.y + old_v.z + old_v.w;  // Should be 1 + 2 + 3 + 4 = 10
}

// run: test_postinc_ivec4() == 10

// ============================================================================
// Pre-decrement (--v) - ivec4
// ============================================================================

int test_predec_ivec4() {
    ivec4 v = ivec4(3, 4, 5, 6);
    ivec4 result = --v;  // Should decrement v to (2, 3, 4, 5), then return (2, 3, 4, 5)
    return result.x + result.y + result.z + result.w;  // Should be 2 + 3 + 4 + 5 = 14
}

// run: test_predec_ivec4() == 14

// ============================================================================
// Post-decrement (v--) - ivec4
// ============================================================================

int test_postdec_ivec4() {
    ivec4 v = ivec4(3, 4, 5, 6);
    ivec4 old_v = v--;
    return old_v.x + old_v.y + old_v.z + old_v.w;  // Should be 3 + 4 + 5 + 6 = 18
}

// run: test_postdec_ivec4() == 18






