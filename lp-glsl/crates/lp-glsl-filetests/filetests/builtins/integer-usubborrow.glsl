// test run
// target riscv32.q32

// ============================================================================
// usubBorrow(): Unsigned subtract with borrow function
// usubBorrow(x, y, out borrow) - unsigned subtract with borrow
// Sets borrow to 0 if x >= y, 1 otherwise
// ============================================================================

uvec2 test_usubborrow_uint_no_borrow() {
    // usubBorrow(5, 3) should return (2, 0)
    uint borrow;
    uint diff = usubBorrow(5u, 3u, borrow);
    return uvec2(diff, borrow);
}

// run: test_usubborrow_uint_no_borrow() == uvec2(2u, 0u) [expect-fail]

uvec2 test_usubborrow_uint_with_borrow() {
    // usubBorrow(3, 5) should return (large_number, 1)
    uint borrow;
    uint diff = usubBorrow(3u, 5u, borrow);
    return uvec2(diff, borrow);
}

// run: test_usubborrow_uint_with_borrow() == uvec2(4294967294u, 1u) [expect-fail]

uvec2 test_usubborrow_uint_zero() {
    // usubBorrow(0, 0) should return (0, 0)
    uint borrow;
    uint diff = usubBorrow(0u, 0u, borrow);
    return uvec2(diff, borrow);
}

// run: test_usubborrow_uint_zero() == uvec2(0u, 0u) [expect-fail]

uvec2 test_usubborrow_uint_equal() {
    // usubBorrow(10, 10) should return (0, 0)
    uint borrow;
    uint diff = usubBorrow(10u, 10u, borrow);
    return uvec2(diff, borrow);
}

// run: test_usubborrow_uint_equal() == uvec2(0u, 0u) [expect-fail]

uvec4 test_usubborrow_uvec2() {
    // usubBorrow with uvec2
    uvec2 borrow;
    uvec2 diff = usubBorrow(uvec2(5u, 3u), uvec2(3u, 5u), borrow);
    return uvec4(diff.x, diff.y, borrow.x, borrow.y);
}

// run: test_usubborrow_uvec2() == uvec4(2u, 4294967294u, 0u, 1u) [expect-fail]




