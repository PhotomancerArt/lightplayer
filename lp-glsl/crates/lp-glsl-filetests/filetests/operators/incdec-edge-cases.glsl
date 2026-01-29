// test run
// target riscv32.q32

int test_incdec_edge_cases() {
    int x = 5;
    ivec2 v = ivec2(1, 2);

    // Test multiple increments in same expression (undefined behavior but should compile)
    int result1 = x++ + x++;  // x becomes 7, result1 is 5 + 6 = 11

    // Test integer vector increment
    ivec2 old_v = v++;  // v becomes ivec2(2, 3), old_v is ivec2(1, 2)

    // Return computed result: result1 + old_v components + v components
    return result1 + old_v.x + old_v.y + v.x + v.y;
}

// run: test_incdec_edge_cases() == 19
