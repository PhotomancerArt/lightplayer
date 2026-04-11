# Milestone 2: Integration

## Goal

Wire `fa_alloc` into `compile_function`, add an `rv32fa` filetest target, and
validate that straight-line functions produce correct results compared to the
cranelift pipeline.

## Suggested Plan Name

`fastalloc3-m2`

## Scope

### In scope

- **Wire `fa_alloc` into `compile.rs`**: replace `rv32::alloc::allocate` call
  with `fa_alloc` allocation, producing `Vec<PInst>` that feeds into the
  existing `Rv32Emitter`
- **`rv32fa` filetest target**: add `Backend::Rv32fa` to `lps-filetests` target
  system so filetests can run against the new allocator
- **Filetest validation**: straight-line filetests pass under `rv32fa`, matching
  cranelift (`rv32`) execution results
- **Error handling**: control flow / call filetests that `rv32fa` can't handle
  yet should fail gracefully with clear error (not panic), annotated as
  `unimplemented` in filetests
- **CLI updates**: `shader-rv32fa` command uses `fa_alloc` path, trace output
  shows real decisions

### Out of scope

- Control flow support — M3
- Call support — M3
- Removing old code — M4

## Key Decisions

- The `rv32fa` filetest target uses the same emulator infrastructure as `rv32lp`.
  The only difference is the allocator.
- `compile_function` switches to `fa_alloc` unconditionally — the old
  `rv32::alloc` path is kept but no longer the default. (It gets removed in M4.)
- Filetests that require control flow or calls are annotated
  `// unimplemented: rv32fa.q32` until M3.

## Deliverables

- Updated `compile.rs` using `fa_alloc` allocation
- `Backend::Rv32fa` in `lps-filetests/src/targets/mod.rs`
- Filetest runner wiring for `rv32fa` target
- Straight-line filetests passing under `rv32fa`
- Updated `shader-rv32fa` CLI pipeline

## Dependencies

- M1 (allocator core): `fa_alloc` produces correct `Vec<PInst>` for
  straight-line code

## Estimated Scope

~200-300 lines of integration/wiring code. Bulk of work is getting filetests
to pass (debugging allocation correctness against cranelift reference).
