# Phase 5: Support Private Global Access Roots

## Scope of Phase

Extend the writable actual resolver to support private global access roots
where VMContext storage can provide a direct address or writeback path. Uniform
roots must be rejected clearly.

In scope:

- Private global scalar/vector/matrix bare roots if the existing global storage
  model supports writeback.
- Private global array elements.
- Private global struct fields and nested fields.
- Private global arrays-of-structs if existing peel/store helpers support the
  shape.
- Clear rejection for uniform globals and uniform-derived access paths.
- Preserve behavior from Phases 1-4.

Out of scope:

- Implementing future `global-future/*` storage classes.
- Changing VMContext global layout.
- Making uniform writes legal.
- Broad uniform block feature work unrelated to write rejection.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than
  improvising.
- Report back: what changed, what was validated, and any deviations from this
  phase plan.

## Implementation Details

Read first:

- `docs/roadmaps/2026-04-24-filetest-q32-cleanup/m9-access-lvalue-out-inout/00-design.md`
- `lp-shader/lps-frontend/src/lower_lvalue.rs`
- `lp-shader/lps-frontend/src/lower_ctx.rs`
- `lp-shader/lps-frontend/src/lower_access.rs`
- `lp-shader/lps-frontend/src/lower_struct.rs`
- `lp-shader/lps-frontend/src/lower_stmt.rs`
- `lp-shader/lps-filetests/filetests/uniform/write-error.glsl`

Global roots are represented in `ctx.global_map` as `GlobalVarInfo` with:

- `byte_offset`: VMContext offset;
- `component_count`;
- `ty`;
- `is_uniform`.

Add resolver support for access chains rooted in `Expression::GlobalVariable`.

Uniform rule:

- If `GlobalVarInfo::is_uniform` is true, reject immediately with a clear
  write error. Prefer the existing wording `"cannot write to uniform variable"`
  unless a more specific existing diagnostic is already used in this area.
- Also reject uniform instance locals and deferred uniform paths if the
  resolver encounters those shapes.

Private global direct-address rule:

- For aggregate subobjects with stable VMContext layout, compute
  `VMCTX_VREG + byte_offset + subobject_offset` and pass that address directly
  when the resolved subobject type is an aggregate compatible with the callee
  pointee type.
- Use existing std430 layout helpers; do not duplicate layout algorithms.

Private global temp/writeback rule:

- For scalar/vector/matrix leaves, use temp/writeback.
- Reuse existing global write helpers:
  - `lower_access::store_through_access()` for global array access where
    practical;
  - `lower_struct::store_struct_path_into_global()` for struct paths;
  - existing bare global store logic in `lower_stmt.rs` if bare scalar/vector
    global roots are supported.

Tests:

- Add or extend a focused function filetest for private global access actuals
  if the current GLSL frontend supports the relevant private global syntax in
  product scope.
- Add uniform negative coverage in or near
  `lp-shader/lps-filetests/filetests/uniform/write-error.glsl`:

```glsl
layout(binding = 0) uniform float u_value;
void set_value(out float x) { x = 1.0; }
float test_uniform_out_actual_rejected() {
    set_value(u_value); // should fail compilation/lowering
    return 0.0;
}
```

Use the repository's existing negative-test annotation style for expected
compile/lowering failures. If the existing filetest harness cannot express this
negative case cleanly, stop and report the limitation rather than inventing a
new convention.

## Validate

Run:

```bash
cargo check -p lps-frontend
cargo test -p lps-frontend
scripts/glsl-filetests.sh --target wasm.q32
scripts/glsl-filetests.sh --target rv32c.q32
scripts/glsl-filetests.sh --target rv32n.q32
```

If any target has unrelated pre-existing failures, report them with enough
detail for the main agent to distinguish them from this phase.
