// test run
// target riscv32.q32

// ============================================================================
// Global Variable Access: Functions can access global variables
// ============================================================================

float global_counter = 0.0;
vec2 global_position = vec2(0.0, 0.0);
bool global_flag = false;

float get_counter() {
    return global_counter;
}

float test_scope_global_read() {
    // Functions can read global variables
    global_counter = 42.0;
    return get_counter();
}

// run: test_scope_global_read() ~= 42.0 [expect-fail]

void increment_counter() {
    global_counter = global_counter + 1.0;
}

void test_scope_global_write() {
    // Functions can write to global variables
    global_counter = 5.0;
    increment_counter();
    increment_counter();
    // global_counter should now be 7.0
}

// run: test_scope_global_write() == 0.0 [expect-fail]

void scale_position(float factor) {
    global_position = global_position * factor;
}

float test_scope_global_modify() {
    // Functions can modify global variables
    global_position = vec2(2.0, 3.0);
    scale_position(2.0);
    return global_position.x + global_position.y;
}

// run: test_scope_global_modify() ~= 10.0 [expect-fail]

void toggle_flag() {
    global_flag = !global_flag;
}

bool get_flag() {
    return global_flag;
}

bool test_scope_global_flag() {
    // Functions can read and write boolean globals
    global_flag = true;
    toggle_flag();
    return get_flag();
}

// run: test_scope_global_flag() == false [expect-fail]

void add_to_counter(float value) {
    global_counter = global_counter + value;
}

void multiply_counter(float factor) {
    global_counter = global_counter * factor;
}

float get_counter_multiple() {
    return global_counter;
}

float test_scope_global_multiple() {
    // Multiple functions accessing same globals
    global_counter = 2.0;
    add_to_counter(3.0);    // counter = 5.0
    multiply_counter(2.0);  // counter = 10.0
    return get_counter_multiple();
}

// run: test_scope_global_multiple() ~= 10.0 [expect-fail]

void move_position(vec2 delta) {
    global_position = global_position + delta;
}

vec2 get_position() {
    return global_position;
}

vec2 test_scope_global_vector() {
    // Functions working with global vectors
    global_position = vec2(10.0, 20.0);
    move_position(vec2(5.0, -3.0));
    return get_position();
}

// run: test_scope_global_vector() ~= vec2(15.0, 17.0) [expect-fail]

float use_local_counter() {
    float global_counter = 99.0; // Shadows global
    return global_counter;
}

float test_scope_global_local_shadow() {
    // Local variables can shadow globals
    global_counter = 123.0;
    float result = use_local_counter(); // Returns 99.0, not 123.0
    return result;
}

// run: test_scope_global_local_shadow() ~= 99.0 [expect-fail]

void accumulate(float value) {
    global_counter = global_counter + value;
}

float test_scope_global_preserve_global() {
    // Global values persist across function calls
    global_counter = 0.0;
    accumulate(5.0);
    accumulate(10.0);
    accumulate(15.0);
    return global_counter;
}

// run: test_scope_global_preserve_global() ~= 30.0 [expect-fail]

void set_state(bool new_state) {
    global_flag = new_state;
}

bool check_and_toggle() {
    bool old_state = global_flag;
    global_flag = !global_flag;
    return old_state;
}

bool test_scope_global_state_machine() {
    // Global state across multiple function calls
    global_flag = true;
    bool first_check = check_and_toggle();  // Returns true, sets to false
    bool second_check = check_and_toggle(); // Returns false, sets to true
    return first_check && !second_check && global_flag;
}

// run: test_scope_global_state_machine() == true [expect-fail]
