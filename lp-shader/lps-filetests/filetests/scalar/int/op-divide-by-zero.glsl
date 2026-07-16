// test run

// ============================================================================
// Integer division edge cases. LPIR follows RISC-V (RV32M) semantics on all
// targets — division never traps (docs/design/lpir/02-core-ops.md):
//   x / 0        == -1
//   INT_MIN / -1 == INT_MIN
// ============================================================================

int div_ints(int a, int b) {
    return a / b;
}

// @unsupported(wgpu.f32)
// run: div_ints(42, 0) == -1
// @unsupported(wgpu.f32)
// run: div_ints(-42, 0) == -1
// @unsupported(wgpu.f32)
// run: div_ints(0, 0) == -1
// @unsupported(wgpu.f32)
// run: div_ints(-2147483648, 0) == -1
// run: div_ints(-2147483648, -1) == -2147483648
// run: div_ints(-2147483648, 1) == -2147483648
// run: div_ints(42, -1) == -42

int test_int_divide_by_zero_local() {
    int a = 7;
    int b = 0;
    return a / b;
}

// wgpu.f32: integer division/modulo by zero is undefined on GPU hardware (Q32 pins device semantics)
// @unsupported(wgpu.f32)
// run: test_int_divide_by_zero_local() == -1

int test_int_min_divide_by_minus_one_local() {
    int minv = -2147483648;
    int b = -1;
    return minv / b;
}

// run: test_int_min_divide_by_minus_one_local() == -2147483648

// Guard idiom: with eager `&&` lowering the division still executes when
// i == 0; it must produce -1 (not trap) so the guarded expression stays false.
int guarded_div_positive(int x, int i) {
    if (i != 0 && (x / i > 0)) {
        return 1;
    }
    return 0;
}

// run: guarded_div_positive(10, 0) == 0
// run: guarded_div_positive(10, 2) == 1
// run: guarded_div_positive(10, -2) == 0
// run: guarded_div_positive(-10, 0) == 0

// Same idiom via `||`: RHS division by zero must not trap when i == 0.
int guarded_div_or(int x, int i) {
    if (i == 0 || x / i > 100) {
        return 1;
    }
    return 0;
}

// run: guarded_div_or(10, 0) == 1
// run: guarded_div_or(1000, 2) == 1
// run: guarded_div_or(10, 2) == 0
