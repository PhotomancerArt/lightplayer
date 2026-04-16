// test run

// ============================================================================
// Global Variable Access from Functions: Accessing globals from user-defined functions
// ============================================================================

float global_counter = 0.0;
vec2 global_position = vec2(0.0, 0.0);
bool global_flag = false;
mat4 global_transform = mat4(1.0);

float _access_read_counter() {
    return global_counter;
}

vec2 _access_read_position() {
    return global_position;
}

float test_access_from_function_read() {
    global_counter = 42.0;
    global_position = vec2(10.0, 20.0);

    return _access_read_counter() + _access_read_position().x + _access_read_position().y;
}

// run: test_access_from_function_read() ~= 72.0

void _access_increment_counter() {
    global_counter = global_counter + 1.0;
}

void _access_update_position(vec2 delta) {
    global_position = global_position + delta;
}

void test_access_from_function_write() {
    global_counter = 5.0;
    global_position = vec2(1.0, 2.0);

    _access_increment_counter();
    _access_increment_counter();
    _access_update_position(vec2(3.0, 4.0));
}

// run: test_access_from_function_write() == 0.0

float test_access_from_function_verify_write() {
    // Verify writes from previous test
    test_access_from_function_write();
    return global_counter + global_position.x + global_position.y;
}

// run: test_access_from_function_verify_write() ~= 17.0

void _access_toggle_flag() {
    global_flag = !global_flag;
}

bool _access_get_flag() {
    return global_flag;
}

bool test_access_from_function_flag() {
    global_flag = true;
    _access_toggle_flag();
    return _access_get_flag();
}

// run: test_access_from_function_flag() == false

void _access_scale_transform(float factor) {
    global_transform = global_transform * factor;
}

mat4 _access_get_transform() {
    return global_transform;
}

mat4 test_access_from_function_matrix() {
    global_transform = mat4(2.0);
    _access_scale_transform(3.0);
    return _access_get_transform();
}

// run: test_access_from_function_matrix() ~= mat4(6.0, 0.0, 0.0, 0.0, 0.0, 6.0, 0.0, 0.0, 0.0, 0.0, 6.0, 0.0, 0.0, 0.0, 0.0, 6.0)

void _access_nested_inner() {
    global_counter = global_counter * 2.0;
}

void _access_nested_outer() {
    _access_nested_inner();
    global_counter = global_counter + 10.0;
}

float test_access_from_function_nested() {
    global_counter = 5.0;
    _access_nested_outer();
    return global_counter;
}

// run: test_access_from_function_nested() ~= 20.0

void _access_move_x(float delta) {
    global_position = vec2(global_position.x + delta, global_position.y);
}

void _access_move_y(float delta) {
    global_position = vec2(global_position.x, global_position.y + delta);
}

vec2 _access_get_position() {
    return global_position;
}

vec2 test_access_from_function_multiple() {
    global_position = vec2(0.0, 0.0);
    _access_move_x(5.0);
    _access_move_y(10.0);
    _access_move_x(3.0);

    return _access_get_position();
}

// run: test_access_from_function_multiple() ~= vec2(8.0, 10.0)
