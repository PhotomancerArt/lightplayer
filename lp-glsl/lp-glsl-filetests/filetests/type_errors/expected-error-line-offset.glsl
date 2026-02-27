// test error
// target riscv32.q32

void test_incomplete_expr() {
    int x =  // expected-error@+1 {{expected '{', found ;}}
        ;
}
