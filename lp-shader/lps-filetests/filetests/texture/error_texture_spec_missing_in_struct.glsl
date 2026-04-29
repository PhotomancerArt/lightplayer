// test compile-error
// target: wasm.q32

// expect-compile-failure: {{missing texture binding spec for `params.gradient`}}

// Struct uniforms with texture fields require a matching dotted texture-spec key.
// This test verifies the compile-time validation catches missing specs for nested textures.

// texture-spec: params.amount format=r16unorm filter=nearest wrap=clamp shape=2d

struct Params {
    float amount;
};
uniform Params params;

float f() {
    // This would need params.gradient if we had texture support in structs
    return params.amount;
}
