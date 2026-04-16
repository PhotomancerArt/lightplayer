// test run

// ============================================================================
// Global Scope: Global variables are visible everywhere
// ============================================================================

float global_counter = 0.0;
vec2 global_position = vec2(0.0, 0.0);
bool global_flag = false;
mat3 global_matrix = mat3(1.0);

void _sg_func1() {
    global_counter = global_counter + 1.0;
}

void _sg_func2() {
    global_position = global_position + vec2(1.0, 1.0);
}

float test_scope_global_visibility() {
    global_counter = 5.0;
    global_position = vec2(10.0, 20.0);

    _sg_func1();
    _sg_func2();

    return global_counter + global_position.x + global_position.y;
}

// run: test_scope_global_visibility() ~= 38.0

bool test_scope_global_persistence() {
    global_flag = true;
    bool first_check = global_flag;
    global_flag = false;
    bool second_check = global_flag;

    return first_check && !second_check;
}

// run: test_scope_global_persistence() == true

void _sg_scale_matrix(float factor) {
    global_matrix = global_matrix * factor;
}

mat3 _sg_get_matrix() {
    return global_matrix;
}

mat3 test_scope_global_matrix() {
    global_matrix = mat3(2.0);
    _sg_scale_matrix(3.0);
    return _sg_get_matrix();
}

// run: test_scope_global_matrix() ~= mat3(6.0, 0.0, 0.0, 0.0, 6.0, 0.0, 0.0, 0.0, 6.0)

void _sg_nested_inner() {
    global_counter = global_counter * 2.0;
}

void _sg_nested_outer() {
    _sg_nested_inner();
    global_counter = global_counter + 5.0;
}

float test_scope_global_nested_functions() {
    global_counter = 3.0;
    _sg_nested_outer();
    return global_counter;
}

// run: test_scope_global_nested_functions() ~= 11.0

void _sg_move_right(float distance) {
    global_position = vec2(global_position.x + distance, global_position.y);
}

void _sg_move_up(float distance) {
    global_position = vec2(global_position.x, global_position.y + distance);
}

vec2 _sg_get_position() {
    return global_position;
}

vec2 test_scope_global_multiple_functions() {
    global_position = vec2(0.0, 0.0);
    _sg_move_right(5.0);
    _sg_move_up(10.0);
    _sg_move_right(3.0);

    return _sg_get_position();
}

// run: test_scope_global_multiple_functions() ~= vec2(8.0, 10.0)

void _sg_increment() {
    global_counter = global_counter + 1.0;
}

void _sg_multiply(float factor) {
    global_counter = global_counter * factor;
}

float _sg_get_value() {
    return global_counter;
}

float test_scope_global_state_machine() {
    global_counter = 1.0;
    _sg_increment();
    _sg_increment();
    _sg_multiply(2.0);
    _sg_increment();

    return _sg_get_value();
}

// run: test_scope_global_state_machine() ~= 7.0
