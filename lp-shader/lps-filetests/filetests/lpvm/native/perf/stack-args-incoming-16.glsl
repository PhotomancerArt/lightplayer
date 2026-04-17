// compile-opt(inline.mode, never)

// test run
//
// Performance: incoming stack parameter load overhead.
// When args exceed a0-a7, each stack arg needs an lw in the prologue.
// This test measures the baseline cost vs stack-loaded cost.

// Stress: 16 scalar args (8 on stack = 8x lw in prologue)
int test_sixteen_args_stack(int a, int b, int c, int d, int e, int f, int g, int h,
                          int i, int j, int k, int l, int m, int n, int o, int p) {
    return a + b + c + d + e + f + g + h + i + j + k + l + m + n + o + p;
}

// run: test_sixteen_args_stack(1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1) == 16
