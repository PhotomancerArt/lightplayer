// test run
// target riscv32.q32

// ============================================================================
// Variable Indexing: Out-of-bounds indices (should trap)
// ============================================================================
//
// NOTE: These tests are currently failing because traps aren't being triggered
// at runtime, even though the trap instructions are being generated correctly
// in the CLIF IR. See emit_bounds_check() in component.rs for details.
// TODO: Fix trap execution in emulator or investigate why traps aren't triggering.
//

bool test_bvec2_index_negative() {
    bvec2 a = bvec2(true, false);
    int i = -1;
    return a[i];
}

// run: test_bvec2_index_negative() == false
// EXPECT_TRAP: vector/matrix index out of bounds

bool test_bvec2_index_too_large() {
    bvec2 a = bvec2(true, false);
    int i = 2;
    return a[i];
}

// run: test_bvec2_index_too_large() == false
// EXPECT_TRAP: vector/matrix index out of bounds

bool test_bvec3_index_negative() {
    bvec3 a = bvec3(true, false, true);
    int i = -1;
    return a[i];
}

// run: test_bvec3_index_negative() == false
// EXPECT_TRAP_CODE: 1

bool test_bvec3_index_too_large() {
    bvec3 a = bvec3(true, false, true);
    int i = 3;
    return a[i];
}

// run: test_bvec3_index_too_large() == false
// EXPECT_TRAP_CODE: 1

bool test_bvec4_index_negative() {
    bvec4 a = bvec4(true, false, true, false);
    int i = -1;
    return a[i];
}

// run: test_bvec4_index_negative() == false
// EXPECT_TRAP: vector/matrix index out of bounds

bool test_bvec4_index_too_large() {
    bvec4 a = bvec4(true, false, true, false);
    int i = 4;
    return a[i];
}

// run: test_bvec4_index_too_large() == false
// EXPECT_TRAP: vector/matrix index out of bounds

int test_ivec2_index_negative() {
    ivec2 a = ivec2(10, 20);
    int i = -1;
    return a[i];
}

// run: test_ivec2_index_negative() == 0
// EXPECT_TRAP_CODE: 1

int test_ivec2_index_too_large() {
    ivec2 a = ivec2(10, 20);
    int i = 2;
    return a[i];
}

// run: test_ivec2_index_too_large() == 0
// EXPECT_TRAP: vector/matrix index out of bounds

float test_vec2_index_negative() {
    vec2 a = vec2(1.5, 2.5);
    int i = -1;
    return a[i];
}

// run: test_vec2_index_negative() == 0.0
// EXPECT_TRAP: vector/matrix index out of bounds

float test_vec2_index_too_large() {
    vec2 a = vec2(1.5, 2.5);
    int i = 2;
    return a[i];
}

// run: test_vec2_index_too_large() == 0.0
// EXPECT_TRAP_CODE: 1

