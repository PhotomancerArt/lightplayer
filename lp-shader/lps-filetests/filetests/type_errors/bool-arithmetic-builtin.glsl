// test error

// `max` / `min` are declared over genType / genIType / genUType only — never
// genBType, so the file must be refused.

bool test_bvec_max() {
    bvec2 a = bvec2(true, false);
    bvec2 b = bvec2(false, true);
    // expected-error@+1 {{arithmetic expects numeric lanes, got BVec2 and BVec2}}
    bvec2 c = max(a, b);
    return c[0];
}
