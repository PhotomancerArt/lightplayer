// test run

// ============================================================================
// findMSB(): Find most significant bit function
// findMSB(value) - returns bit number of most significant bit
// For positive: most significant one bit
// For negative: most significant zero bit
// Returns -1 if value is 0 or -1
// ============================================================================

int test_findmsb_int_zero() {
    // findMSB(0) should be -1
    return findMSB(0);
}

// @unsupported(wgpu.f32)
// run: test_findmsb_int_zero() == -1

int test_findmsb_int_one() {
    // findMSB(1) should be 0 (bit 0 is the most significant one)
    return findMSB(1);
}

// @unsupported(wgpu.f32)
// run: test_findmsb_int_one() == 0

int test_findmsb_int_two() {
    // findMSB(2) should be 1 (bit 1 is the most significant one: 0b10)
    return findMSB(2);
}

// @unsupported(wgpu.f32)
// run: test_findmsb_int_two() == 1

int test_findmsb_int_neg_one() {
    // findMSB(-1) should be -1 (all bits set)
    return findMSB(-1);
}

// @unsupported(wgpu.f32)
// run: test_findmsb_int_neg_one() == -1

int test_findmsb_int_negative() {
    // findMSB(-2) should be 0 (bit 0 is the most significant zero bit in ...11111110)
    return findMSB(-2);
}

// @unsupported(wgpu.f32)
// run: test_findmsb_int_negative() == 0

int test_findmsb_int_large() {
    // 2147483648 overflows int to INT_MIN (0x80000000); findMSB of a negative
    // value is its most significant zero bit: 30
    return findMSB(2147483648);
}

// findMSB(-2)==0 / findMSB(int 2^31)==30 per GLSL semantics (naga const-eval agrees); prior expectations were wrong, the Q32 @broken lines masked correct results.
// @unsupported(wgpu.f32)
// run: test_findmsb_int_large() == 30

uint test_findmsb_uint_zero() {
    // findMSB(uint(0)) should be -1, but returns uint so check against 4294967295u (-1 as uint)
    return findMSB(0u);
}

// @unsupported(wgpu.f32)
// run: test_findmsb_uint_zero() == 4294967295u

uint test_findmsb_uint_one() {
    // findMSB(uint(1)) should be 0
    return findMSB(1u);
}

// @unsupported(wgpu.f32)
// run: test_findmsb_uint_one() == 0u

uint test_findmsb_uint_large() {
    // findMSB(uint(2147483648)) should be 31
    return findMSB(2147483648u);
}

// @unsupported(wgpu.f32)
// run: test_findmsb_uint_large() == 31u

ivec2 test_findmsb_ivec2() {
    // findMSB with ivec2
    return findMSB(ivec2(0, 2));
}

// wgpu.f32: naga validator rejects the assembled unit (std430 uniform blocks / unsized array constructors are invalid on the GPU tier)
// @unsupported(wgpu.f32)
// run: test_findmsb_ivec2() == ivec2(-1, 1)



