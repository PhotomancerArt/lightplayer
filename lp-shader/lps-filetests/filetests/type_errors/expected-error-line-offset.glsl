// test error

void test_incomplete_expr() {
    int x =  // expected-error@+1 {{expected '{', found ;}}
        ;
}
