# Phase 3: Support Local Aggregate Access Direct Addresses

## Scope of Phase

Extend the writable actual resolver to support local aggregate access actuals,
using direct addresses when the destination is a stable aggregate byte region
and temp/writeback for scalar/vector/matrix leaves.

In scope:

- Local array element actuals, including dynamic and const subscripts.
- Local struct field actuals.
- Nested local struct field actuals.
- Local arrays-of-structs, including fields of array elements.
- Preserve behavior from Phases 1 and 2.

Out of scope:

- Pointer-argument roots.
- Private global roots.
- Uniform roots beyond explicit rejection if encountered.
- Changing array index clamping semantics.
- Changing aggregate pointer ABI.

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
- `lp-shader/lps-frontend/src/lower_array.rs`
- `lp-shader/lps-frontend/src/lower_array_multidim.rs`
- `lp-shader/lps-frontend/src/lower_struct.rs`
- `lp-shader/lps-frontend/src/lower_stmt.rs`
- `lp-shader/lps-frontend/src/naga_util.rs`

Add resolver arms for local aggregate roots:

- `Access` / `AccessIndex` chains rooted in local arrays.
- `AccessIndex` chains rooted in local structs.
- Array-of-struct chains recognized by
  `lower_struct::peel_arrayofstruct_chain()`.

Direct-address rule:

- If the resolved actual type is an aggregate and the storage root is
  `AggregateSlot::Local`, compute the address as `base + byte_offset` and pass
  it directly.
- Use existing helpers where possible:
  - `aggregate_storage_base_vreg()`
  - `array_element_address()` / `array_element_address_with_field_offset()`
  - existing struct layout/member path helpers in `lower_struct.rs`
- Ensure the callee pointee type and resolved subobject type are layout
  compatible before using direct address.

Temp/writeback rule:

- If the resolved local aggregate path ends in a scalar/vector/matrix leaf,
  use the temp/writeback model from Phase 2.
- Reuse existing `load_struct_path_from_local()`,
  `store_struct_path_into_local()`, array element load/store helpers, and
  array-of-struct store helpers where possible.

Important details:

- Preserve existing clamped indexing behavior for array dynamic indices.
- Do not write through `AggregateSlot::ParamReadOnly`.
- Avoid duplicating std430 layout math if an existing layout helper can answer
  the offset.
- Keep any new address computation helper private to `lower_lvalue.rs` unless
  another module genuinely needs it.

Suggested tests for this phase:

- `out` to `arr[1]`.
- `inout` to `arr[i]`.
- `out` to `s.field`.
- `inout` to `s.inner.value`.
- `inout` to `points[i].x`.

These can be staged in the final focused filetest if Phase 6 will consolidate
test files, but this phase should validate at least representative local
aggregate access behavior before reporting completion.

## Validate

Run:

```bash
cargo check -p lps-frontend
cargo test -p lps-frontend
scripts/glsl-filetests.sh --target wasm.q32
```

If focused filetest filtering is available, run the new/affected function
file(s) for `wasm.q32` and include the exact command in the report.
