// test run

// One prototype + definition per file. Prototype uses the same parameter names as the
// definition so lowering matches (all-unnamed prototypes like `float add(float,float)` mis-parse).

float add(float a, float b);

float add(float a, float b) {
    return a + b;
}

float test_param_unnamed_simple() {
    return add(3.0, 4.0);
}

// @unimplemented(jit.q32)
// run: test_param_unnamed_simple() ~= 7.0
