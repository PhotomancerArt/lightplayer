// compile-opt(inline.mode, never)

// test run

// ============================================================================
// Early returns in while loops
// ============================================================================

int find_with_while(int[5] arr) {
    int i = 0;
    while (i < 5) {
        if (arr[i] > 10) {
            return arr[i];
        }
        i = i + 1;
    }
    return -1;
}

int test_return_in_while() {
    int[5] data = int[5](1, 2, 15, 4, 5);
    return find_with_while(data);
}

// run: test_return_in_while() == 15

// ============================================================================
// Nested while with early return
// ============================================================================

int nested_while_return(int[3] outer, int[3] inner) {
    int i = 0;
    while (i < 3) {
        if (outer[i] > 0) {
            int j = 0;
            while (j < 3) {
                if (inner[j] > outer[i]) {
                    return inner[j];
                }
                j = j + 1;
            }
        }
        i = i + 1;
    }
    return -1;
}

int test_nested_while_return() {
    int[3] a = int[3](5, 1, 2);
    int[3] b = int[3](1, 10, 3);
    return nested_while_return(a, b);
}

// run: test_nested_while_return() == 10
