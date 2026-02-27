// test run
// target riscv32.q32

// Spec: variables.adoc §4.3.3.1 "Constant Expressions"
// Constructors whose arguments are constant expressions.

const vec2 UNIT_VECTOR = vec2(1.0, 0.0);
const vec2 SCALED_VECTOR = UNIT_VECTOR * 2.0;
const vec3 UP_VECTOR = vec3(0.0, 1.0, 0.0);
const vec3 RIGHT_VECTOR = vec3(1.0, 0.0, 0.0);
const vec3 FORWARD_VECTOR = vec3(0.0, 0.0, 1.0);
const mat2 IDENTITY = mat2(1.0, 0.0, 0.0, 1.0);
const mat2 SCALED_IDENTITY = IDENTITY * 2.0;

vec2 test_constructors_vector() {
    return SCALED_VECTOR + vec2(1.0, 1.0);
}

// run: test_constructors_vector() ~= vec2(3.0, 1.0)

vec3 test_constructors_vector_ops() {
    return UP_VECTOR + RIGHT_VECTOR + FORWARD_VECTOR;
}

// run: test_constructors_vector_ops() ~= vec3(1.0, 1.0, 1.0)

mat2 test_constructors_matrix() {
    return SCALED_IDENTITY;
}

// run: test_constructors_matrix() ~= mat2(2.0, 0.0, 0.0, 2.0)
