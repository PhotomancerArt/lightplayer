// test run
// target riscv32.q32

// ============================================================================
// Forward Declarations: Declare before define, multiple prototypes allowed
// ============================================================================

// Forward declarations
float compute_area(float width, float height);
vec2 transform_point(vec2 point, mat2 matrix);
void initialize_data(out float[3] data);

float test_forward_declare_simple() {
    // Call function before its definition using forward declaration
    return compute_area(4.0, 5.0);
}

// run: test_forward_declare_simple() ~= 20.0

vec2 test_forward_declare_vector() {
    // Call vector function with forward declaration
    vec2 point = vec2(1.0, 2.0);
    mat2 transform = mat2(1.0, 0.0, 0.0, 1.0);
    return transform_point(point, transform);
}

// run: test_forward_declare_vector() ~= vec2(1.0, 2.0)

void test_forward_declare_array() {
    // Call function that initializes array
    float[3] data;
    initialize_data(data);
    // Data should be initialized to [1.0, 2.0, 3.0]
}

// run: test_forward_declare_array() == 0.0

float test_forward_declare_multiple_calls() {
    // Multiple calls to forward declared functions
    float area1 = compute_area(2.0, 3.0);
    float area2 = compute_area(5.0, 6.0);
    return area1 + area2;
}

// run: test_forward_declare_multiple_calls() ~= 36.0

float test_forward_declare_in_expression() {
    // Forward declared function in expression
    return compute_area(3.0, 4.0) * 2.0;
}

// run: test_forward_declare_in_expression() ~= 48.0 [expect-fail]

// Multiple forward declarations (allowed)
float add_numbers(float a, float b);
float add_numbers(float a, float b); // Duplicate prototype OK

float test_forward_declare_duplicate() {
    return add_numbers(7.0, 8.0);
}

// run: test_forward_declare_duplicate() ~= 15.0

// Forward declaration with different parameter names (OK)
vec2 scale_vector(vec2 input, float factor);

vec2 test_forward_declare_different_names() {
    vec2 v = vec2(2.0, 3.0);
    return scale_vector(v, 2.0);
}

// run: test_forward_declare_different_names() ~= vec2(4.0, 6.0)

// ============================================================================
// Function Definitions (implementing the forward declarations)
// ============================================================================

float compute_area(float width, float height) {
    return width * height;
}

vec2 transform_point(vec2 point, mat2 matrix) {
    return matrix * point;
}

void initialize_data(out float[3] data) {
    data[0] = 1.0;
    data[1] = 2.0;
    data[2] = 3.0;
}

float add_numbers(float a, float b) {
    return a + b;
}

vec2 scale_vector(vec2 input, float factor) {
    return input * factor;
}
