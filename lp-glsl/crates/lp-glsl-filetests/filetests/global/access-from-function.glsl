// test run
// target riscv32.q32

// ============================================================================
// Global Variable Access from Functions: Accessing globals from user-defined functions
// ============================================================================

float global_counter = 0.0;
vec2 global_position = vec2(0.0, 0.0);
bool global_flag = false;
mat4 global_transform = mat4(1.0);

float test_access_from_function_read() {
    // Function reading global variables
    float read_counter() {
        return global_counter;
    }

    vec2 read_position() {
        return global_position;
    }

    global_counter = 42.0;
    global_position = vec2(10.0, 20.0);

    return read_counter() + read_position().x + read_position().y;
}

// run: test_access_from_function_read() ~= 72.0 [expect-fail]

void test_access_from_function_write() {
    // Function writing to global variables
    void increment_counter() {
        global_counter = global_counter + 1.0;
    }

    void update_position(vec2 delta) {
        global_position = global_position + delta;
    }

    global_counter = 5.0;
    global_position = vec2(1.0, 2.0);

    increment_counter();
    increment_counter();
    update_position(vec2(3.0, 4.0));
}

// run: test_access_from_function_write() == 0.0 [expect-fail]

float test_access_from_function_verify_write() {
    // Verify writes from previous test
    test_access_from_function_write();
    return global_counter + global_position.x + global_position.y;
}

// run: test_access_from_function_verify_write() ~= 15.0 [expect-fail]

bool test_access_from_function_flag() {
    // Function manipulating boolean global
    void toggle_flag() {
        global_flag = !global_flag;
    }

    bool get_flag() {
        return global_flag;
    }

    global_flag = true;
    toggle_flag();
    return get_flag();
}

// run: test_access_from_function_flag() == false [expect-fail]

mat4 test_access_from_function_matrix() {
    // Function working with matrix global
    void scale_transform(float factor) {
        global_transform = global_transform * factor;
    }

    mat4 get_transform() {
        return global_transform;
    }

    global_transform = mat4(2.0);
    scale_transform(3.0);
    return get_transform();
}

// run: test_access_from_function_matrix() ~= mat4(6.0, 0.0, 0.0, 0.0, 0.0, 6.0, 0.0, 0.0, 0.0, 0.0, 6.0, 0.0, 0.0, 0.0, 0.0, 6.0) [expect-fail]

float test_access_from_function_nested() {
    // Nested functions accessing globals
    void outer_func() {
        void inner_func() {
            global_counter = global_counter * 2.0;
        }

        inner_func();
        global_counter = global_counter + 10.0;
    }

    global_counter = 5.0;
    outer_func();
    return global_counter;
}

// run: test_access_from_function_nested() ~= 20.0 [expect-fail]

vec2 test_access_from_function_multiple() {
    // Multiple functions accessing same globals
    void move_x(float delta) {
        global_position.x = global_position.x + delta;
    }

    void move_y(float delta) {
        global_position.y = global_position.y + delta;
    }

    vec2 get_position() {
        return global_position;
    }

    global_position = vec2(0.0, 0.0);
    move_x(5.0);
    move_y(10.0);
    move_x(3.0);

    return get_position();
}

// run: test_access_from_function_multiple() ~= vec2(8.0, 10.0) [expect-fail]
