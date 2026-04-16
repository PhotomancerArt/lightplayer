# Stage VI: Integration + Scalar Filetest Validation

## Goal

Wire the full pipeline end-to-end and pass all existing scalar filetests.
Verify the web-demo still renders.

## Suggested plan name

`lpir-stage-vi`

## Scope

**In scope:**

- Wire `lps-frontend::compile()` → LPIR lowering → WASM emission
  (Q32 inside the WASM emitter) in the public API
- Pass all existing scalar filetests (wasm.q32 + wasm.float where applicable)
    - `filetests/scalar/arithmetic/`
    - `filetests/scalar/bool/`
    - `filetests/scalar/builtins/`
    - `filetests/scalar/lpfx/`
- Verify web-demo (scalar rainbow.glsl) still renders
- Debug and fix any discrepancies between interpreter results and WASM results
- Remove or gate any dead code paths from the old emitter

**Out of scope:**

- Vector filetests (Phase II follow-on)
- Cranelift integration (future)
- Performance optimization

## Deliverables

- All scalar filetests passing via LPIR pipeline
- Web-demo rendering correctly
- CI green

## Dependencies

- Stages I–V must be complete.

## Estimated scope

~200 lines of glue + debugging. Main effort is integration testing and
fixing edge cases.
