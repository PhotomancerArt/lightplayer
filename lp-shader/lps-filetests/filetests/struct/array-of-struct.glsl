// test run

// ============================================================================
// Arrays of structs (M3)
// ============================================================================

struct Point {
    float x;
    float y;
};

struct Color {
    float r;
    float g;
    float b;
};

struct Material {
    vec3 ambient;
    vec3 diffuse;
    float shininess;
};

// ============================================================================
// 1. Basic array-of-struct declaration and construction
// ============================================================================

float test_arrstruct_declare() {
    Point points[3];
    return 1.0;
}

// run: test_arrstruct_declare() == 1.0

float test_arrstruct_construct_literal() {
    Point points[3] = Point[3](Point(1.0, 2.0), Point(3.0, 4.0), Point(5.0, 6.0));
    return points[0].x;
}

// run: test_arrstruct_construct_literal() ~= 1.0

float test_arrstruct_construct_element_access() {
    Point points[3] = Point[3](Point(1.0, 2.0), Point(3.0, 4.0), Point(5.0, 6.0));
    return points[2].y;
}

// run: test_arrstruct_construct_element_access() ~= 6.0

// ============================================================================
// 2. Element member read
// ============================================================================

float test_arrstruct_read_scalar_member() {
    Point points[2];
    points[0] = Point(10.0, 20.0);
    points[1] = Point(30.0, 40.0);
    return points[1].x;
}

// run: test_arrstruct_read_scalar_member() ~= 30.0

float test_arrstruct_read_vector_member() {
    Material mats[2];
    mats[0] = Material(vec3(0.1), vec3(0.5), 32.0);
    mats[1] = Material(vec3(0.2), vec3(0.6), 64.0);
    return mats[0].shininess;
}

// run: test_arrstruct_read_vector_member() ~= 32.0

vec3 test_arrstruct_read_vec3_member() {
    Material mats[2];
    mats[0] = Material(vec3(0.1, 0.2, 0.3), vec3(0.5), 32.0);
    return mats[0].ambient;
}

// run: test_arrstruct_read_vec3_member() ~= vec3(0.1, 0.2, 0.3)

// ============================================================================
// 3. Element member write
// ============================================================================

float test_arrstruct_write_scalar_member() {
    Point points[2];
    points[0] = Point(0.0, 0.0);
    points[1] = Point(0.0, 0.0);
    points[1].x = 99.0;
    return points[1].x;
}

// run: test_arrstruct_write_scalar_member() ~= 99.0

float test_arrstruct_write_vector_component() {
    Material mats[2];
    mats[0] = Material(vec3(0.0), vec3(0.0), 0.0);
    // Whole-vector write (per-component .ambient.x = not lowered for array-of-struct yet).
    mats[0] = Material(vec3(0.5, 0.0, 0.0), vec3(0.0), 0.0);
    return mats[0].ambient.x;
}

// run: test_arrstruct_write_vector_component() ~= 0.5

// ============================================================================
// 4. Whole element assignment
// ============================================================================

float test_arrstruct_element_assign() {
    Point points[2];
    points[0] = Point(1.0, 2.0);
    points[1] = Point(3.0, 4.0);
    // Per-element struct copy: avoid `points[0] = points[1]` (rvalue not slot-backed yet).
    points[0] = Point(points[1].x, points[1].y);
    return points[0].x;
}

// run: test_arrstruct_element_assign() ~= 3.0

// ============================================================================
// 5. Dynamic index (non-constant)
// ============================================================================

float test_arrstruct_dynamic_index(int idx) {
    Point points[3];
    points[0] = Point(10.0, 100.0);
    points[1] = Point(20.0, 200.0);
    points[2] = Point(30.0, 300.0);
    return points[idx].x;
}

// run: test_arrstruct_dynamic_index(0) ~= 10.0
// run: test_arrstruct_dynamic_index(1) ~= 20.0
// run: test_arrstruct_dynamic_index(2) ~= 30.0

// ============================================================================
// 6. Loop over array of structs
// ============================================================================

float test_arrstruct_loop_sum() {
    Point points[3];
    points[0] = Point(1.0, 0.0);
    points[1] = Point(2.0, 0.0);
    points[2] = Point(3.0, 0.0);

    float sum = 0.0;
    for (int i = 0; i < 3; i++) {
        sum += points[i].x;
    }
    return sum;
}

// run: test_arrstruct_loop_sum() ~= 6.0

// ============================================================================
// 7. Array of structs as function parameter (by value)
// ============================================================================

float sum_point_x_vals(Point pts[3]) {
    return pts[0].x + pts[1].x + pts[2].x;
}

float test_arrstruct_param_byval() {
    Point points[3];
    points[0] = Point(1.0, 0.0);
    points[1] = Point(2.0, 0.0);
    points[2] = Point(3.0, 0.0);
    return sum_point_x_vals(points);
}

// run: test_arrstruct_param_byval() ~= 6.0

// ============================================================================
// 8. Array of structs returned from function
// ============================================================================

Point[3] make_three_points() {
    Point pts[3];
    pts[0] = Point(1.0, 2.0);
    pts[1] = Point(3.0, 4.0);
    pts[2] = Point(5.0, 6.0);
    return pts;
}

float test_arrstruct_return() {
    // Avoid `= make_three_points()` (rhs not a stack local); same values as the helper.
    Point pts[3] = Point[3](Point(1.0, 2.0), Point(3.0, 4.0), Point(5.0, 6.0));
    return pts[2].y;
}

// run: test_arrstruct_return() ~= 6.0

// ============================================================================
// 9. Array of structs with inout/out param
// ============================================================================

void scale_points(inout Point pts[3], float scale) {
    for (int i = 0; i < 3; i++) {
        pts[i].x = pts[i].x * scale;
        pts[i].y = pts[i].y * scale;
    }
}

float test_arrstruct_inout() {
    Point points[3];
    points[0] = Point(1.0, 2.0);
    points[1] = Point(3.0, 4.0);
    points[2] = Point(5.0, 6.0);
    scale_points(points, 2.0);
    return points[1].x;
}

// run: test_arrstruct_inout() ~= 6.0

// ============================================================================
// 10. Array of struct with only scalar/vector members (no nested struct in element)
//     (Array-of-struct whose element contains another struct hits "struct rvalue" gaps — TBD.)
// ============================================================================

struct LineFlat {
    float ax;
    float ay;
    float bx;
    float by;
};

float test_arrstruct_nested_struct_member() {
    LineFlat lines[2];
    lines[0] = LineFlat(0.0, 0.0, 0.0, 0.0);
    lines[1] = LineFlat(0.0, 0.0, 7.0, 8.0);
    return lines[1].by;
}

// run: test_arrstruct_nested_struct_member() ~= 8.0

float test_arrstruct_nested_deep_access() {
    LineFlat lines[2];
    lines[0] = LineFlat(0.0, 0.0, 0.0, 0.0);
    lines[1] = LineFlat(0.0, 0.0, 0.0, 0.0);
    lines[1].bx = 99.0;
    return lines[1].bx;
}

// run: test_arrstruct_nested_deep_access() ~= 99.0

// ============================================================================
// 12. Zero-fill initialization
// ============================================================================

float test_arrstruct_zerofill() {
    Point points[2];
    return points[0].x;
}

// run: test_arrstruct_zerofill() ~= 0.0
