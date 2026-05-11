# Phase 6: Add Focused Filetests and Retire Markers

## Scope of Phase

Consolidate acceptance coverage for M9 and remove stale filetest markers for
access-lvalue `out` / `inout` behavior that is now supported.

In scope:

- Add a focused supported-behavior filetest for writable access actuals.
- Add or update uniform negative coverage for rejected writable uniform
  actuals.
- Re-run `function/edge-lvalue-out.glsl` and retire `@broken` markers that are
  now fixed.
- Do not add new `jit.q32` annotations.
- Preserve known unrelated failures, especially the unrelated
  `function/call-order.glsl` `rv32n.q32` failure.

Out of scope:

- New implementation work beyond small fixes required by tests added in this
  phase.
- Solving unrelated filetest failures.
- Changing q32 unsupported policy.
- Broad annotation cleanup outside files touched by this milestone.

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
- `lp-shader/lps-filetests/filetests/function/edge-lvalue-out.glsl`
- `lp-shader/lps-filetests/filetests/function/param-out-array.glsl`
- `lp-shader/lps-filetests/filetests/array/of-struct/out-param.glsl`
- `lp-shader/lps-filetests/filetests/array/of-struct/inout-param.glsl`
- `lp-shader/lps-filetests/filetests/uniform/write-error.glsl`

Create:

- `lp-shader/lps-filetests/filetests/function/access-lvalue-out-inout.glsl`

The focused function test should cover representative supported actuals:

- bare local regression;
- array element `out`;
- array element `inout`;
- struct field `out`;
- nested struct field `inout`;
- arrays-of-structs field `out` or `inout`;
- vector lane `inout`;
- matrix column or cell `inout`, matching the Naga expression shape that the
  implementation supports;
- pointer-argument root wrapper, e.g. a function receiving `inout` aggregate
  and passing a subobject to another callee;
- private global access actual if Phase 5 implemented support for the syntax.

Keep tests concise. Prefer several small functions with simple return values
over one large test that is difficult to triage.

Update:

- `lp-shader/lps-filetests/filetests/function/edge-lvalue-out.glsl`

Remove `@broken(wasm.q32)`, `@broken(rv32c.q32)`, and `@broken(rv32n.q32)`
markers only for rows that now pass on those targets. If a row still fails for
an unrelated reason, leave its marker and document the reason in the phase
report.

Uniform negative coverage:

- Add a targeted case to `uniform/write-error.glsl` if the file already uses a
  suitable expected-failure convention.
- If no clear convention exists, add a small adjacent negative test file only if
  the harness supports expected compile/lowering errors.
- If the harness cannot express this negative case, do not fake it with a
  disabled/commented test. Report the gap.

Do not add new `jit.q32` annotations. If an existing touched row contains
deprecated `jit.q32` annotations, remove or leave them only according to the
filetest convention used elsewhere in the touched file; do not broaden the
plan into a deprecated-target cleanup.

## Validate

Run:

```bash
cargo check -p lps-frontend
cargo test -p lps-frontend
scripts/glsl-filetests.sh --target wasm.q32
scripts/glsl-filetests.sh --target rv32c.q32
scripts/glsl-filetests.sh --target rv32n.q32
```

Also run any narrower filetest command the harness supports for:

- `function/access-lvalue-out-inout.glsl`
- `function/edge-lvalue-out.glsl`
- the uniform write-error file touched in this phase

Report the exact commands and any remaining markers left intentionally.
