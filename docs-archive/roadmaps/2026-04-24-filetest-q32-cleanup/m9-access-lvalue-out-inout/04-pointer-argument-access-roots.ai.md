# Phase 4: Support Pointer-Argument Access Roots

## Scope of Phase

Extend the writable actual resolver to support `Access` / `AccessIndex` actuals
rooted in current-function pointer arguments. This covers a function receiving
an `out` / `inout` aggregate or vector/matrix and passing one of its writable
subobjects to another callee.

In scope:

- Pointer-argument vector lanes.
- Pointer-argument matrix columns/cells where existing lowering supports the
  shape.
- Pointer-argument array elements.
- Pointer-argument struct fields and nested fields.
- Preserve behavior from Phases 1-3.

Out of scope:

- Private global roots.
- Uniform roots except clear rejection.
- Read-only `in` aggregate optimization changes.
- Changing how function pointer arguments are represented in LPIR.

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
- `lp-shader/lps-frontend/src/lower_stmt.rs`
- `lp-shader/lps-frontend/src/lower_struct.rs`

Pointer arguments are tracked in `LowerCtx::pointer_args`, and their runtime
address is `ctx.arg_vregs_for(arg_i)?[0]`.

Add resolver support for access chains whose root is
`Expression::FunctionArgument(arg_i)` and `ctx.pointer_args.contains_key(&arg_i)`.

Direct-address rule:

- For aggregate subobjects inside an addressable pointer-argument buffer,
  compute `arg_base + byte_offset` and pass the address directly when the
  resolved subobject type is an aggregate compatible with the callee pointee
  type.
- This should cover array elements that are aggregate values and struct fields
  that are aggregate values.

Temp/writeback rule:

- For scalar/vector/matrix leaves, use a temporary slot and write back into the
  pointer-argument buffer after the call.
- Reuse the existing pointer-argument store paths in `lower_access.rs` and
  `lower_stmt.rs` where possible:
  - vector lane writes through parameter pointers;
  - array element writes through `AggregateSlot::Param`;
  - struct member writes using the pointer-argument struct store logic.

Read-only guard:

- Do not treat by-value read-only `in` aggregates as writable roots. Those are
  represented through `AggregateSlot::ParamReadOnly`, not `pointer_args`, but
  explicitly reject if a path resolution ever discovers that storage kind.

Suggested tests:

```glsl
void set_float(out float x) { x = 7.0; }

void wrapper_array(inout float arr[2]) {
    set_float(arr[1]);
}

void wrapper_vec(inout vec3 v) {
    set_float(v.y);
}

struct Inner { float value; };
struct Outer { Inner inner; };

void wrapper_struct(inout Outer o) {
    set_float(o.inner.value);
}
```

Add these to the focused access-lvalue file or an equivalent test file that
Phase 6 will finalize.

## Validate

Run:

```bash
cargo check -p lps-frontend
cargo test -p lps-frontend
scripts/glsl-filetests.sh --target wasm.q32
```

If focused filetest filtering is available, run the new/affected function
file(s) for `wasm.q32` and include the exact command in the report.
