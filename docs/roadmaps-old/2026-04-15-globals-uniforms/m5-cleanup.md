# Milestone 5: Cleanup + Validation

## Goal

Integration validation across all milestones. Cleanup temporary scaffolding,
remove stubs, fix warnings. All validation commands pass.

## Suggested Plan Name

`globals-uniforms-m5`

## Scope

### In scope

- **Remove VmContext stubs**: Delete `unimplemented!("Milestone 2")` from
  `get_global`, `set_global`, `get_uniform` — replace with real
  implementations or remove if superseded by the offset-based approach.

- **Documentation**: Update `VmContext` doc comments to reflect the actual
  layout. Update `LpvmInstance` trait docs to describe globals/uniforms
  lifecycle.

- **Warning cleanup**: Fix any new warnings introduced across milestones.

- **Validation commands**: All must pass:
  ```bash
  cargo test -p lps-filetests
  cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu
  cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf \
    --profile release-esp32 --features esp32c6,server
  cargo check -p fw-emu --target riscv32imac-unknown-none-elf \
    --profile release-emu
  cargo check -p lpa-server
  cargo test -p lpa-server --no-run
  ```

- **Filetest audit**: Verify no `@unimplemented` remains on global filetests
  for backends that should now support them.

- **Edge case tests**: Add tests for:
  - Shader with no globals and no uniforms (zero overhead path).
  - Shader with uniforms but no globals (no snapshot/reset needed).
  - Shader with globals but no uniforms.
  - Global initializer that calls a helper function.
  - Multiple globals of different types (layout alignment correctness).

### Out of scope

- New features beyond globals/uniforms.
- Performance optimization beyond the `globals_size == 0` fast path.

## Deliverables

- Clean build with no new warnings.
- All validation commands passing.
- Updated documentation.
- Edge case filetests.

## Dependencies

- M4 (engine + filetests): everything wired up and basic tests passing.

## Estimated Scope

~100-200 lines of cleanup/docs/tests.

## Agent Execution Notes

This milestone is a straightforward cleanup pass. A single agent session:

1. Run all validation commands, collect any failures.
2. Fix failures.
3. Audit `filetests/global/` for remaining `@unimplemented` tags.
4. Add edge case tests.
5. Clean up doc comments and remove stubs.
6. Run validation commands again to confirm clean.

Stop and request review if any validation failure is non-trivial.
