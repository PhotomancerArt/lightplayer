// test error
// Uniform global must not be passed as `out` / `inout` writable actual.

layout(binding = 0) uniform float u_value;

void set_value(out float x) {
    x = 1.0;
}

float test_uniform_out_actual_rejected() {
    set_value(u_value); // expected-error {{cannot write to uniform variable}}
    return 0.0;
}
