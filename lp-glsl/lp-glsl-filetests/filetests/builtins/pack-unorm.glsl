// test run
// target riscv32.q32

// ============================================================================
// packUnorm*(): Pack normalized values functions
// packUnorm2x16(vec2) - pack 2 floats to uint
// packUnorm4x8(vec4) - pack 4 floats to uint
// ============================================================================

uint test_packunorm2x16_zeros() {
    // packUnorm2x16(vec2(0.0, 0.0)) should pack to 0
    return packUnorm2x16(vec2(0.0, 0.0));
}

// run: test_packunorm2x16_zeros() == 0u [expect-fail]

uint test_packunorm2x16_ones() {
    // packUnorm2x16(vec2(1.0, 1.0)) should pack to all bits set for 16-bit values
    return packUnorm2x16(vec2(1.0, 1.0));
}

// run: test_packunorm2x16_ones() == 4294967295u [expect-fail]

uint test_packunorm2x16_half() {
    // packUnorm2x16(vec2(0.5, 0.5)) should pack to half values
    return packUnorm2x16(vec2(0.5, 0.5));
}

// run: test_packunorm2x16_half() == 2147516416u [expect-fail]

uint test_packunorm4x8_zeros() {
    // packUnorm4x8(vec4(0.0, 0.0, 0.0, 0.0)) should pack to 0
    return packUnorm4x8(vec4(0.0, 0.0, 0.0, 0.0));
}

// run: test_packunorm4x8_zeros() == 0u [expect-fail]

uint test_packunorm4x8_ones() {
    // packUnorm4x8(vec4(1.0, 1.0, 1.0, 1.0)) should pack to all bits set
    return packUnorm4x8(vec4(1.0, 1.0, 1.0, 1.0));
}

// run: test_packunorm4x8_ones() == 4294967295u [expect-fail]

uint test_packunorm4x8_quarters() {
    // packUnorm4x8(vec4(0.25, 0.25, 0.25, 0.25))
    return packUnorm4x8(vec4(0.25, 0.25, 0.25, 0.25));
}

// run: test_packunorm4x8_quarters() == 67372036u [expect-fail]




