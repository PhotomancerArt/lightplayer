// test error

// expected-error {{texture binding spec}}

// A dotted texture-spec must name a sampler field of the struct uniform.
// `params.amount` is a float, so the spec matches no sampler2D and is rejected —
// the same rule as error_extra_texture_spec.glsl, reached through a struct.
//
// The struct uniform needs `layout(binding = …)`; naga rejects a bare
// `uniform Params params;` before any texture validation runs.

// texture-spec: params.amount format=r16unorm filter=nearest wrap=clamp shape=2d

struct Params {
    float amount;
};
layout(binding = 0) uniform Params params;

float f() {
    return params.amount;
}
