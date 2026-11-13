// test run

// GLSL ES 3.00 §4.1.3 integer literals: decimal, octal (a leading 0) and
// hexadecimal (0x / 0X), each with or without the `u` suffix.

int test_hex_int() {
    return 0xFF;
}

// run: test_hex_int() == 255

int test_hex_upper_prefix() {
    return 0X1a;
}

// run: test_hex_upper_prefix() == 26

uint test_hex_uint() {
    return 0xF8u;
}

// run: test_hex_uint() == 248u

uint test_hex_uint_max() {
    return 0xFFFFFFFFu;
}

// run: test_hex_uint_max() == 4294967295u

int test_hex_int_all_bits() {
    // An int literal past INT_MAX keeps its bits.
    return 0xFFFFFFFF;
}

// run: test_hex_int_all_bits() == -1

int test_hex_negated() {
    return -0x10;
}

// run: test_hex_negated() == -16

int test_octal_int() {
    return 017;
}

// run: test_octal_int() == 15

uint test_octal_uint() {
    return 010u;
}

// run: test_octal_uint() == 8u

int test_zero() {
    return 0;
}

// run: test_zero() == 0

uint test_hex_in_expression() {
    uint index = 0xABu;
    return (index & 0xF8u) >> 3;
}

// run: test_hex_in_expression() == 21u
