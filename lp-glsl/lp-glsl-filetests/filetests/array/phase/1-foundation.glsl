// test run
// target riscv32.q32

// Phase 1: Foundation - Basic 1D scalar arrays with literal int sizes
// Stack allocation, pointer-based storage, basic read/write access

// Test 1: Basic array declaration
int test_declaration() {
    int arr[5];
    return 0; // Just verify declaration compiles
}

// Test 2: Verify array type is stored correctly (try to use array in expression)
int test_type_stored() {
    int arr[5];
    // This should work if array type is stored correctly
    // We'll just return 0 for now since we can't test indexing yet
    return 0;
}

// Test 3: Basic array write (assignment) - this is where it fails
int test_write() {
    int arr[5];
    arr[0] = 10;
    return 0; // Just verify assignment compiles
}

// Test 4: Basic array read
int test_read() {
    int arr[5];
    arr[0] = 10; // Initialize first
    int x = arr[0]; // Read it back
    return x; // Should return 10
}

// Test 5: Multiple writes
int test_multiple_writes() {
    int arr[5];
    arr[0] = 10;
    arr[1] = 20;
    arr[2] = 30;
    return 0; // Just verify multiple assignments compile
}

// Test 6: Multiple reads
int test_multiple_reads() {
    int arr[5];
    arr[0] = 10;
    arr[2] = 30;
    arr[4] = 50;
    int x = arr[0];
    int y = arr[2];
    int z = arr[4];
    return x + y + z; // Should be 10 + 30 + 50 = 90
}

// Test 7: Full phase1 test (all together)
int phase1() {
    // Basic declaration
    int arr[5];
    
    // Basic write
    arr[0] = 10;
    arr[1] = 20;
    arr[2] = 30;
    arr[3] = 40;
    arr[4] = 50;
    
    // Basic read
    int x = arr[0];
    int y = arr[2];
    int z = arr[4];
    
    // Return sum to verify values
    return x + y + z; // Should be 10 + 30 + 50 = 90
}

// run: test_declaration() == 0
// run: test_type_stored() == 0
// run: test_write() == 0
// run: test_read() == 10
// run: test_multiple_writes() == 0
// run: test_multiple_reads() == 90
// run: phase1() == 90