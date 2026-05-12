# Phase 3: Loader And Example Slice

## Scope

- Teach `ProjectLoader` to attach `ComputeShaderNode`.
- Register authored compute bindings where they already fit the binding model.
- Add a small test project or in-memory loader test for `kind = "shader/compute"`.

## Implementation Notes

- Keep the existing basic example unchanged unless the compute example is tiny
  and clearly separate.
- Prefer a targeted in-memory `TestFs` style test if existing support makes it
  cheap.

## Validation

- `cargo test -p lpc-engine compute -- --nocapture`
- `cargo check -p lpc-engine`
