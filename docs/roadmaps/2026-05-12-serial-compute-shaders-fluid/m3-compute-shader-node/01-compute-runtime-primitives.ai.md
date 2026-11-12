# Phase 1: Compute Runtime Primitives

## Scope

- Add an engine-level `LpComputeShader` trait.
- Extend `LpGraphics` with `compile_compute_shader`.
- Implement it for host, wasm guest, and native RV32 graphics backends.
- Keep pixel shader APIs unchanged.

## Implementation Notes

- Use `compute_desc_from_model_def` at the node layer, not in the backend.
- The backend method should accept `lp_shader::CompileComputeDesc`.
- Error mapping should match existing `compile_shader` style.

## Validation

- `cargo check -p lpc-engine`
- Existing M2 test: `cargo test -p lpc-engine compute_def_header_and_runtime_descriptor_execute -- --nocapture`
