// test error

// `max` / `min` are declared over genType / genIType / genUType only — never
// genBType. naga reports an ambiguous overload; lps-glsl reports
// "arithmetic expects numeric lanes, got BVec2 and BVec2" (pinned by unit
// tests in that crate). Both must refuse the file.

bool test_bvec_max() {
    bvec2 a = bvec2(true, false);
    bvec2 b = bvec2(false, true);
    // expected-error@+2 {{Ambiguous best function for 'max'}}
    // expected-error@+1 {{Can't resolve type: BuiltinArgumentsInvalid("Max")}}
    bvec2 c = max(a, b);
    return c[0];
}
