// compile-opt(inline.mode, never)

// test run
//
// Call clobber + spill slot correctness: sequential calls, evictions during arg
// setup, callee-saved pressure, sret with live scalars, stack outgoing args.

float add(float a, float b) {
    return a + b;
}

// 1) Four scalar results live across four calls (t-reg spill / clobber evictions)
float test_call_chain_spill() {
    float a = add(1.0, 0.0);
    float b = add(2.0, 0.0);
    float c = add(3.0, 0.0);
    float d = add(4.0, 0.0);
    return a + b + c + d;
}

// run: test_call_chain_spill() ~= 10.0

// 2) Return in a0 fed as argument to the next call
float test_chain_passthrough() {
    float x = add(1.0, 2.0);
    float y = add(x, 4.0);
    float z = add(y, 8.0);
    return z;
}

// run: test_chain_passthrough() ~= 15.0

// 3) Many live scalars (s-regs) plus a call that may evict during arg setup
float test_callee_saved_eviction() {
    float a = 1.0;
    float b = 2.0;
    float c = 3.0;
    float d = 4.0;
    float e = 5.0;
    float f = 6.0;
    float g = 7.0;
    float h = 8.0;
    float i = 9.0;
    float j = 10.0;
    float k = 11.0;
    float r = add(a, b);
    return r + c + d + e + f + g + h + i + j + k;
}

// run: test_callee_saved_eviction() ~= 66.0

// 4) sret aggregate return while scalars stay live
vec4 helper_vec4_broadcast(float x) {
    return vec4(x, x, x, x);
}

float test_sret_with_live_values() {
    float a = 1.0;
    float b = 2.0;
    vec4 v = helper_vec4_broadcast(3.0);
    return a + b + v.x;
}

// run: test_sret_with_live_values() ~= 6.0

// 5) Same value used as call arg twice while it must stay live
float test_overlap_arg_live() {
    float x = 10.0;
    float a = add(x, 1.0);
    float b = add(x, 2.0);
    return a + b;
}

// run: test_overlap_arg_live() ~= 23.0

// 6) Nine float args (one stack overflow) plus a live scalar across the call
float big_call_with_live(float a, float b, float c, float d,
                          float e, float f, float g, float h, float i) {
    return a + b + c + d + e + f + g + h + i;
}

float test_stack_args_plus_spill() {
    float live = 100.0;
    float r = big_call_with_live(1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0);
    return r + live;
}

// run: test_stack_args_plus_spill() ~= 145.0

// 7) Two vec2-returning calls; first result must survive the second
vec2 make_vec2(float x, float y) {
    return vec2(x, y);
}

vec2 test_interleaved_vec2() {
    vec2 a = make_vec2(1.0, 2.0);
    vec2 b = make_vec2(3.0, 4.0);
    return a + b;
}

// run: test_interleaved_vec2() ~= vec2(4.0, 6.0)
