// test run
// target riscv32.q32

// ============================================================================
// packHalf2x16(): Pack half precision function
// packHalf2x16(vec2) - pack 2 floats to uint (half precision)
// ============================================================================

uint test_packhalf2x16_zeros() {
    // packHalf2x16(vec2(0.0, 0.0)) should pack to 0
    return packHalf2x16(vec2(0.0, 0.0));
}

// run: test_packhalf2x16_zeros() == 0u [expect-fail]

uint test_packhalf2x16_ones() {
    // packHalf2x16(vec2(1.0, 1.0)) should pack to half precision ones
    return packHalf2x16(vec2(1.0, 1.0));
}

// run: test_packhalf2x16_ones() == 100664832u [expect-fail]

uint test_packhalf2x16_half() {
    // packHalf2x16(vec2(0.5, 0.5)) should pack to half precision halves
    return packHalf2x16(vec2(0.5, 0.5));
}

// run: test_packhalf2x16_half() == 50331648u [expect-fail]

uint test_packhalf2x16_neg_one() {
    // packHalf2x16(vec2(-1.0, 1.0)) should pack negative and positive
    return packHalf2x16(vec2(-1.0, 1.0));
}

// run: test_packhalf2x16_neg_one() == 100664832u [expect-fail]

uint test_packhalf2x16_two() {
    // packHalf2x16(vec2(2.0, 2.0)) should pack to half precision twos
    return packHalf2x16(vec2(2.0, 2.0));
}

// run: test_packhalf2x16_two() == 100663296u [expect-fail]

uint test_packhalf2x16_small() {
    // packHalf2x16(vec2(0.1, 0.1)) should pack small values
    return packHalf2x16(vec2(0.1, 0.1));
}

// run: test_packhalf2x16_small() == 50331648u [expect-fail]




