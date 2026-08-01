// test run

// Uniform ARRAY OF STRUCTS. Distinct from `uniform/array.glsl` (uniform
// arrays of scalars/vectors, dynamically indexable) and from
// `array/of-struct/` (function-local struct arrays): a uniform struct
// array's element address must be resolvable at lower time, so members are
// read through CONSTANT indices — the idiom `examples/events/shader.glsl`
// and `examples/effects/meteor` both carry.
//
// Regression origin (2026-07-29): the meteor effect shipped a helper that
// indexed this array with a runtime parameter. It compiled on every
// Naga-frontend target and failed only on `Frontend::Lp` lowering
// ("AccessIndex: struct value behind Load: Access base has no uniform
// element addr"), so both the engine render test and CI passed while the
// browser sim failed.

struct Particle {
    uint id;
    vec2 pos;
    vec3 color;
    float intensity;
};

layout(binding = 0) uniform Particle particles[4];

// Members pass into the helper as scalars/vectors; the helper never
// indexes the uniform array itself.
vec3 accumulate(vec3 accum, uint id, vec2 pos, vec3 color, float intensity) {
    if (id == 0u) {
        return accum;
    }
    return accum + color * intensity + vec3(pos.x, pos.y, 0.0);
}

vec3 test_uniform_aos_constant_index_through_helper() {
    vec3 accum = vec3(0.0, 0.0, 0.0);
    accum = accumulate(accum, particles[0].id, particles[0].pos, particles[0].color, particles[0].intensity);
    accum = accumulate(accum, particles[1].id, particles[1].pos, particles[1].color, particles[1].intensity);
    accum = accumulate(accum, particles[2].id, particles[2].pos, particles[2].color, particles[2].intensity);
    accum = accumulate(accum, particles[3].id, particles[3].pos, particles[3].color, particles[3].intensity);
    return accum;
}

// run: test_uniform_aos_constant_index_through_helper() ~= vec3(0.0, 0.0, 0.0)

// Direct constant-index member reads at the top level.
float test_uniform_aos_direct_member_read() {
    return particles[0].intensity + particles[3].pos.x + float(particles[2].id);
}

// run: test_uniform_aos_direct_member_read() ~= 0.0
