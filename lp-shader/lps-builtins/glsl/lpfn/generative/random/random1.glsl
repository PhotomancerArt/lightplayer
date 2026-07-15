// lpfn_random(float) — 1D sin-based random (canonical f32 semantics).
//
// Canonical GLSL source for the LightPlayer `lpfn_random(float, uint)`
// builtin, matching `src/builtins/lpfn/generative/random/random1_q32.rs`.
//
// Classic sin-hash: fract(sin(x) * 43758.5453). Credit: David Hoskins /
// the widely used one-liner distributed by LYGIA under MIT for random.glsl
// (see docs/reports/2026-03-31-lpfx-license-audit.md).
//
// Seed semantics (LightPlayer): the Q32 implementation adds the raw seed
// word to the Q16.16 angle, i.e. the phase shifts by seed * 2^-16 radians.
// The canonical reproduces that: any nonzero seed decorrelates the output
// because the sin-hash amplifies phase differences by ~43758.
//
// NOTE: this function is chaotic by construction (fract of a large multiple
// of sin). Finite-precision implementations (Q16.16 device math vs f32)
// necessarily decorrelate pointwise; conformance for the random family is
// statistical, not pointwise. See lps-filetests conformance harness.

float lpfn_random(float x, uint seed) {
    float combined = x + float(seed) * (1.0 / 65536.0);
    return fract(sin(combined) * 43758.5453);
}
