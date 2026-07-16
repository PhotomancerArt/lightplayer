// test run

// ============================================================================
// Integer remainder edge cases. LPIR follows RISC-V (RV32M) semantics on all
// targets — remainder never traps (docs/design/lpir/02-core-ops.md):
//   x % 0        == x
//   INT_MIN % -1 == 0
// ============================================================================

int mod_ints(int a, int b) {
    return a % b;
}

// @unsupported(wgpu.f32)
// run: mod_ints(42, 0) == 42
// @unsupported(wgpu.f32)
// run: mod_ints(-42, 0) == -42
// run: mod_ints(0, 0) == 0
// @unsupported(wgpu.f32)
// run: mod_ints(-2147483648, 0) == -2147483648
// run: mod_ints(-2147483648, -1) == 0
// run: mod_ints(-2147483648, 1) == 0
// run: mod_ints(42, -5) == 2

int test_int_modulo_by_zero_local() {
    int a = 7;
    int b = 0;
    return a % b;
}

// wgpu.f32: integer division/modulo by zero is undefined on GPU hardware (Q32 pins device semantics)
// @unsupported(wgpu.f32)
// run: test_int_modulo_by_zero_local() == 7

int test_int_min_modulo_by_minus_one_local() {
    int minv = -2147483648;
    int b = -1;
    return minv % b;
}

// run: test_int_min_modulo_by_minus_one_local() == 0

// Guard idiom: the eager RHS `x % i` must not trap when i == 0.
int guarded_mod(int x, int i) {
    if (i != 0 && (x % i == 0)) {
        return 1;
    }
    return 0;
}

// run: guarded_mod(10, 0) == 0
// run: guarded_mod(10, 5) == 1
// run: guarded_mod(10, 3) == 0
