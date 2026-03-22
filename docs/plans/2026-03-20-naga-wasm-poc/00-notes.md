# Naga WASM POC — Notes

## Scope

Minimal spike crate (`spikes/naga-wasm-poc/`) that demonstrates the full path:

```
GLSL source → Naga GLSL frontend → Naga IR → Q32 transform → WASM emission → wasmtime execution
```

For a trivial shader like `float main(float a, float b) { return a + b; }`.

The goal is NOT a production backend. It's a proof-of-concept to validate:
1. Naga's GLSL frontend parses our style of shaders
2. Naga IR is walkable for WASM emission
3. Q32 transform on Naga IR is feasible
4. Local allocation from the expression arena works
5. The whole path produces correct, runnable WASM

## Current state

- Naga is checked out at `/Users/yona/dev/photomancer/oss/wgpu`
- Naga is `#![no_std]`, GLSL frontend has no `std` usage outside feature-gated error display
- Naga's GLSL frontend depends on `pp-rs` (preprocessor), no_std status unclear
- Existing WASM backend (`lp-glsl-wasm`) uses wasmtime in tests — same pattern here
- Workspace already has `wasmtime = "42"` and `wasm-encoder = "0.245"` as deps

## Questions

### Q1: Crate location and workspace membership?

Spike crate at `spikes/naga-wasm-poc/`. Should it be a workspace member or
standalone?

**Suggestion**: Add to workspace members. This lets it share workspace dep
versions (wasmtime, wasm-encoder). Easy to remove later.

**Answer**: Yes, workspace member at `spikes/naga-wasm-poc/`. Shares workspace
dep versions. Easy to remove later.

### Q2: Naga dependency — path to local checkout or crates.io?

Naga v29 is in the local wgpu checkout. We could use `path = "../oss/wgpu/naga"`
or pull from crates.io.

**Suggestion**: Use path dep to local checkout. Lets us inspect/debug Naga
internals during the spike. Easier than pinning a crates.io version.

**Answer**: **Implemented with crates.io `naga` 29** so the repo builds without a sibling `oss/wgpu` tree. A path dep remains valid for local debugging (`[patch.crates-io]` or `path = "../../../oss/wgpu/naga"` from `spikes/naga-wasm-poc`).

### Q3: Float mode first, or jump straight to Q32?

The spike could start with float-mode WASM (f32 ops, no transform) and then add
Q32, or go directly to Q32.

**Suggestion**: Start with float. Get `a + b` compiling and running as f32 WASM
first. Then add the Q32 transform as a second step. Two phases, each
independently testable.

**Answer**: Float first, then Q32. Two phases.

### Q4: How minimal is the GLSL?

For the spike, do we support:
- (a) Just `float main(float a, float b) { return a + b; }` — one function, scalars, one binary op
- (b) Also vec2/vec3 — to prove vector scalarization works
- (c) Also a builtin like smoothstep — to prove the builtin dispatch path

**Answer**: (a) scalars only. Vectors and builtins are well-understood from
existing backend — the spike validates the Naga → WASM path itself.

### Q5: no_std — does it matter for the spike?

**Answer**: Yes. The spike lib must be `#![no_std]` to validate that Naga's
`glsl-in` actually compiles under no_std. This is one of the primary questions
the spike answers. If pp-rs or anything else in Naga's GLSL path pulls in std,
we find out immediately and know a fork is needed. Tests themselves can use std
(wasmtime requires it).
