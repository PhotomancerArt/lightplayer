// test run
// target riscv32.q32

// ============================================================================
// Global Scope: Global variables are visible everywhere
// ============================================================================

float global_counter = 0.0;
vec2 global_position = vec2(0.0, 0.0);
bool global_flag = false;
mat3 global_matrix = mat3(1.0);
int global_array[3] = int[3](1, 2, 3);

float test_scope_global_visibility() {
    // Global variables visible from any function
    void func1() {
        global_counter = global_counter + 1.0;
    }

    void func2() {
        global_position = global_position + vec2(1.0, 1.0);
    }

    global_counter = 5.0;
    global_position = vec2(10.0, 20.0);

    func1();
    func2();

    return global_counter + global_position.x + global_position.y;
}

// run: test_scope_global_visibility() ~= 27.0 [expect-fail]

bool test_scope_global_persistence() {
    // Global scope persists across function calls
    void set_flag() {
        global_flag = true;
    }

    void clear_flag() {
        global_flag = false;
    }

    bool get_flag() {
        return global_flag;
    }

    set_flag();
    bool first_check = get_flag();  // should be true
    clear_flag();
    bool second_check = get_flag(); // should be false

    return first_check && !second_check;
}

// run: test_scope_global_persistence() == true [expect-fail]

mat3 test_scope_global_matrix() {
    // Global matrix accessible from functions
    void scale_matrix(float factor) {
        global_matrix = global_matrix * factor;
    }

    mat3 get_matrix() {
        return global_matrix;
    }

    global_matrix = mat3(2.0);
    scale_matrix(3.0);
    return get_matrix();
}

// run: test_scope_global_matrix() ~= mat3(6.0, 0.0, 0.0, 0.0, 6.0, 0.0, 0.0, 0.0, 6.0) [expect-fail]

int test_scope_global_array() {
    // Global array accessible from functions
    void modify_array() {
        global_array[0] = 10;
        global_array[1] = 20;
        global_array[2] = 30;
    }

    int sum_array() {
        return global_array[0] + global_array[1] + global_array[2];
    }

    modify_array();
    return sum_array();
}

// run: test_scope_global_array() == 60 [expect-fail]

float test_scope_global_nested_functions() {
    // Global variables accessible from nested functions
    void outer() {
        void inner() {
            global_counter = global_counter * 2.0;
        }

        inner();
        global_counter = global_counter + 5.0;
    }

    global_counter = 3.0;
    outer();
    return global_counter;
}

// run: test_scope_global_nested_functions() ~= 11.0 [expect-fail]

vec2 test_scope_global_multiple_functions() {
    // Multiple functions accessing the same globals
    void move_right(float distance) {
        global_position.x = global_position.x + distance;
    }

    void move_up(float distance) {
        global_position.y = global_position.y + distance;
    }

    vec2 get_position() {
        return global_position;
    }

    global_position = vec2(0.0, 0.0);
    move_right(5.0);
    move_up(10.0);
    move_right(3.0);

    return get_position();
}

// run: test_scope_global_multiple_functions() ~= vec2(8.0, 10.0) [expect-fail]

float test_scope_global_state_machine() {
    // Global state maintained across function calls
    void increment() {
        global_counter = global_counter + 1.0;
    }

    void multiply(float factor) {
        global_counter = global_counter * factor;
    }

    float get_value() {
        return global_counter;
    }

    global_counter = 1.0;
    increment();  // 2.0
    increment();  // 3.0
    multiply(2.0); // 6.0
    increment();  // 7.0

    return get_value();
}

// run: test_scope_global_state_machine() ~= 7.0 [expect-fail]
