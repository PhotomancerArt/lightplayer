// test run

// ============================================================================
// Function Overloading: genuinely overloaded local functions (shared name).
// Regression test for the wasm backend, which used to emit one export per IR
// function under its raw GLSL name — overloads produced duplicate export
// names and the module failed wasm validation.
//
// The other backends still break on genuine overloads (verified 2026-07-09),
// hence the @broken annotations:
// - rv32c.q32: cranelift `declare` keys functions by name, so the
//   second overload is rejected as an incompatible redeclaration.
// - rv32n.q32: lpvm-native hits a regalloc internal error
//   (isa/rv32/emit.rs:850).
// - rv32lpn.q32: the LpsGlsl frontend resolves overloads by name and loses
//   the second definition's parameters ("unknown name `v`").
// - xtn.f32 / xtlpn.f32 (added M8): the same overload confusion surfaces in
//   the Xtensa return path — "3 direct return words exceed RET_REGS (wider
//   returns use sret)" at isa/xt/emit.rs. It is NOT a vec3-return bug: vec3
//   returns work on xtn.f32 everywhere else in the corpus (verified against
//   operators/postinc-vec3, array/index-nested, lpfn/lp_saturate and
//   uniform/struct). The sret decision is simply made from the wrong
//   overload's signature.
// ============================================================================

float pick(float x) {
    return x * 2.0;
}

vec3 pick(vec3 v) {
    return v * 3.0;
}

vec4 pick(vec4 v) {
    return v * 4.0;
}

float test_overload_scalar_vs_vector() {
    // 2*2 + 3*2 + 4*3 = 4 + 6 + 12 = 22
    return pick(2.0) + pick(vec3(1.0, 2.0, 3.0)).y + pick(vec4(1.0, 2.0, 3.0, 4.0)).z;
}

// genuine overloads work only on the wasm backend (see the header for each backend)
// @broken(float_mode=q32, backend!=wasm)
// the same backend limits as the q32 row above, in the other float mode
// @broken(float_mode=f32, backend=rv32n)
// @broken(float_mode=f32, backend=xtn)
// run: test_overload_scalar_vs_vector() ~= 22.0

float combine(float a) {
    return a + 1.0;
}

float combine(float a, float b) {
    // Calls the one-arg overload from inside another overload.
    return combine(a) + b;
}

float test_overload_arity_and_nested_call() {
    // (1+1) + ((2+1)+3) = 2 + 6 = 8
    return combine(1.0) + combine(2.0, 3.0);
}

// genuine overloads work only on the wasm backend (see the header for each backend)
// @broken(float_mode=q32, backend!=wasm)
// the same backend limits as the q32 row above, in the other float mode
// @broken(float_mode=f32, backend=rv32n)
// @broken(float_mode=f32, backend=xtn)
// run: test_overload_arity_and_nested_call() ~= 8.0
