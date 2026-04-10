// test run
//
// Performance: spill/reload density in tight computation sequences.
// The greedy allocator spills when registers exhausted, causing sw/lw pairs.
// This test measures spill overhead in compute-heavy patterns.

// Helper to force computation without optimizing away
int identity(int x) {
    return x;
}

// Baseline: 8 values, all fit in registers (no spills expected)
int test_eight_values_no_spill() {
    int a = identity(1);
    int b = identity(2);
    int c = identity(3);
    int d = identity(4);
    int e = identity(5);
    int f = identity(6);
    int g = identity(7);
    int h = identity(8);
    // Chain forces all values to stay live
    return a + b + c + d + e + f + g + h;
}

// Stress: 20 values, exceeds register file (spills required)
// Each spilled value needs sw after def, lw before use
int test_twenty_values_spill() {
    int a = identity(1);
    int b = identity(2);
    int c = identity(3);
    int d = identity(4);
    int e = identity(5);
    int f = identity(6);
    int g = identity(7);
    int h = identity(8);
    int i = identity(9);
    int j = identity(10);
    int k = identity(11);
    int l = identity(12);
    int m = identity(13);
    int n = identity(14);
    int o = identity(15);
    int p = identity(16);
    int q = identity(17);
    int r = identity(18);
    int s = identity(19);
    int t = identity(20);
    return a + b + c + d + e + f + g + h + i + j + k + l + m + n + o + p + q + r + s + t;
}

// run: test_eight_values_no_spill() == 36
// run: test_twenty_values_spill() == 210
