// test run

// ============================================================================
// Inliner: callee with nested if / for / break / continue (remap stress).
// ============================================================================

int sum_evens_with_cap(int n) {
    int total = 0;
    for (int i = 0; i < n; i++) {
        if (i > 100) {
            break;
        }
        if ((i % 2) == 1) {
            continue;
        }
        total = total + i;
    }
    return total;
}

int test_inline_control_flow_sum() {
    return sum_evens_with_cap(10) + sum_evens_with_cap(5);
}

// 0+2+4+6+8 = 20; 0+2+4 = 6 -> 26
// run: test_inline_control_flow_sum() == 26

int mixed_loop(int n, int skip_below) {
    int acc = 0;
    for (int j = 0; j < n; j++) {
        if (j < skip_below) {
            continue;
        }
        if (j > 50) {
            break;
        }
        if ((j % 3) == 0) {
            acc = acc + j;
        } else {
            acc = acc - 1;
        }
    }
    return acc;
}

int test_inline_control_flow_mixed() {
    return mixed_loop(12, 2) + sum_evens_with_cap(4);
}

// mixed_loop(12,2)=11; sum_evens_with_cap(4)=0+2=2 -> 13
// run: test_inline_control_flow_mixed() == 13
