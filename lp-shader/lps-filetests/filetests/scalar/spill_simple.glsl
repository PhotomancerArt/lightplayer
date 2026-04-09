// test run
//
// Simple spill test with scalar integers only.
// Uses many independent variables to force register exhaustion.

int test_spill_many_ints() {
    int a = 1;
    int b = 2;
    int c = 3;
    int d = 4;
    int e = 5;
    int f = 6;
    int g = 7;
    int h = 8;
    int i = 9;
    int j = 10;
    int k = 11;
    int l = 12;
    int m = 13;
    int n = 14;
    int o = 15;
    int p = 16;
    int q = 17;
    int r = 18;
    int s = 19;
    int t = 20;
    int u = 21;
    int v = 22;
    int w = 23;
    int x = 24;
    int y = 25;
    int z = 26;
    return a + b + c + d + e + f + g + h + i + j + k + l + m + n + o + p + q + r + s + t + u + v + w + x + y + z;
}

// run: test_spill_many_ints() == 351
