// test run
// target riscv32.q32

// ============================================================================
// unpackHalf2x16(): Unpack half precision function
// unpackHalf2x16(uint) - unpack uint to vec2 (half precision)
// ============================================================================

vec2 test_unpackhalf2x16_zero() {
    // unpackHalf2x16(0) should unpack to vec2(0.0, 0.0)
    return unpackHalf2x16(0u);
}

// run: test_unpackhalf2x16_zero() ~= vec2(0.0, 0.0) [expect-fail]

vec2 test_unpackhalf2x16_ones() {
    // unpackHalf2x16 half precision ones
    return unpackHalf2x16(100664832u);
}

// run: test_unpackhalf2x16_ones() ~= vec2(1.0, 1.0) [expect-fail]

vec2 test_unpackhalf2x16_half() {
    // unpackHalf2x16 half precision halves
    return unpackHalf2x16(50331648u);
}

// run: test_unpackhalf2x16_half() ~= vec2(0.5, 0.5) [expect-fail]

vec2 test_unpackhalf2x16_neg_one() {
    // unpackHalf2x16 negative and positive
    return unpackHalf2x16(100664832u);
}

// run: test_unpackhalf2x16_neg_one() ~= vec2(-1.0, 1.0) [expect-fail]

vec2 test_unpackhalf2x16_two() {
    // unpackHalf2x16 half precision twos
    return unpackHalf2x16(100663296u);
}

// run: test_unpackhalf2x16_two() ~= vec2(2.0, 2.0) [expect-fail]

vec2 test_unpackhalf2x16_small() {
    // unpackHalf2x16 small values
    return unpackHalf2x16(50331648u);
}

// run: test_unpackhalf2x16_small() ~= vec2(0.1, 0.1) [expect-fail]




