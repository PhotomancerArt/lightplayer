# M5 Uniform and Global Memory — Notes

## Goal

Fix typed global array stores, forward-reference initialization,
uniform default reads, and readonly/write diagnostics.

## Current Findings

- `global/type-array.glsl` likely has a clear root cause:
  `store_through_access` handles local arrays/vectors/mats and pointer
  arguments, but not `Expression::GlobalVariable` as the base of an
  `Access`. Loads for global/uniform access exist, but subscript stores
  to global arrays fail with `store through Access: unsupported base`.
- Whole-value `Store` to a bare non-uniform global is handled elsewhere;
  subscript/field stores take a different path.
- Uniform Section C rows (**resolved**): `defaults` combined-float expectation
  was arithmetic vs zero defaults; other files needed `100u`/`1u` assertions
  where the runner produced `U32`; pipeline/write-error expectations fixed for
  all-uniforms-zero.
- Research on `layout(binding = 0)`:
  - Naga's GLSL frontend requires a `layout(binding=X)` qualifier for
    globals in `AddressSpace::Uniform` / storage / handle spaces; missing
    binding produces the semantic error "uniform/buffer blocks require
    layout(binding=X)".
  - Our lowering currently ignores the binding number. `compute_global_layout`
    builds `uniforms_type` from the Naga global variables' names/types and
    computes std430 offsets in declaration/global iteration order.
  - Host/runtime writes are name/path based:
    `LpvmInstance::set_uniform(path, value)` resolves `path` in
    `LpsModuleSig::uniforms_type`; `LpsPxShader::apply_uniforms` iterates
    `uniforms_type.members` and calls `set_uniform(name, value)`.
  - Old design notes explicitly say GLSL `layout(binding = N)` slots are
    not wired up, and examples use `layout(binding = 0)` on every uniform
    syntactically while accessing uniforms by name.
  - Conclusion: repeated `layout(binding = 0)` is syntactic compatibility
    with Naga, not a semantic slot/binding model in LightPlayer today.
- `global/forward-reference.glsl` (**resolved**): Naga duplicate `GlobalVariable`
  handles for the same logical global; layout must merge by `(name, space)` so
  loads and `__shader_init` alias one region. Early tests that assumed
  “uninitialized” reads before the initializer line were updated to match
  link-time init (initializers before any function runs).
- `function/call-order.glsl` is cross-cutting because it combines
  argument evaluation order with global mutation.
  M2 investigation recommends deferring it: the remaining rv32n failure
  is an `InvalidMemoryAccess` likely tied to native calls/globals/stack
  behavior rather than q32 numeric parity.
- If M3 adds generic l-value support, M5 still owns the global/vmctx
  storage side.

## Questions For User

- In uniform filetests, is repeated `layout(binding = 0)` intentional
  for the harness as a virtual sequential uniform block, or should those
  tests move toward one explicit uniform block? **Research:** Naga
  requires a binding qualifier, but LightPlayer ignores binding values
  and uses name/path-based uniform layout. Repeated `binding = 0` is
  syntactic compatibility, not an intended slot model. Still needs a
  product decision on whether to keep that source shape or introduce a
  preprocessing/front-end path that avoids requiring bindings.
  **Answered:** Keep the dummy binding syntax for now. It is baggage to
  revisit later; M5 should not solve author-facing uniform syntax.
- For forward-reference globals, should all globals be visible at link
  time with initializers applied before any function runs, or is a
  stricter define-before-use subset acceptable? **Answered:** Yes,
  globals should be visible at link time with initializers applied
  before any function runs.
- Confirm that uniform struct-array-field work stays on the aggregate
  roadmap unless it reappears in the Section C corpus.

## Implementation Notes

- Align memory fixes with the aggregate slot-backed model.
- Keep `global-future/*` out of this milestone unless the user
  explicitly expands product scope.
- Add non-uniform global `Access` store symmetry with existing global
  access load behavior; uniform stores should remain rejected unless
  product scope changes.
- Build a per-file table separating frontend lowering, vmctx init, host
  marshal, and readonly validation issues.
- Do not interpret repeated `layout(binding = 0)` as overlapping storage.
  Current semantics are name/path based and declaration-layout based.
- Keep existing dummy `layout(binding = 0)` source shape in filetests
  for now. Do not introduce a preprocessing/front-end path to remove it
  in this cleanup roadmap.

## Validation

- Targeted uniform/global filetests.
- Key files:
  `global/type-array.glsl`, `global/forward-reference.glsl`,
  `uniform/defaults.glsl`, `uniform/no-init.glsl`,
  `uniform/pipeline.glsl`, `uniform/readonly.glsl`, and
  `uniform/write-error.glsl`.
- Final `just test-filetests`.
