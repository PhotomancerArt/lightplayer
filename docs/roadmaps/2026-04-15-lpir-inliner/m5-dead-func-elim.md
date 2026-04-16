# M5 — Dead Function Elimination

Remove functions from the module that have zero remaining local call sites
and aren't in the root set. Separate from inlining — the inliner (M3)
never deletes functions.

## Motivation

After inlining, helper functions have had their bodies copied into all
callers. The originals still exist in the module and still get compiled.
In production (single entry point), these are pure waste — removing them
saves compile time and code size.

Filetests don't use this pass (every function is potentially callable by
the test harness).

## API

```rust
pub struct DeadFuncElimResult {
    pub functions_removed: usize,
}

/// Remove functions with zero local call sites that aren't in `roots`.
pub fn dead_func_elim(
    module: &mut LpirModule,
    roots: &[usize],  // indices into module.functions
) -> DeadFuncElimResult {
    // ...
}
```

`roots` identifies the externally callable functions. Everything else is
a candidate for removal if it has zero remaining local call sites.

## Algorithm

1. **Count local call sites.** Walk every function body, count how many
   `Call` ops target each local function.

2. **Mark reachable.** Starting from roots, transitively mark any function
   that has a non-zero call count. (After full inlining, local call counts
   should be zero for all non-import callees. But partial inlining or
   disabled inlining could leave some calls.)

3. **Remove unmarked.** Delete functions not in the reachable set. With
   stable `FuncId` (M0), deletion doesn't invalidate any references.

4. **Update module signature.** Remove corresponding `LpsFnSig` entries
   from `LpsModuleSig`.

## Integration

### Production path

The engine knows the shader entry point name. Before compilation:

```rust
if options.opt.is_enabled(OptPass::DeadFuncElim) {
    let root_indices = find_roots_by_name(&ir, &["main"]);
    lpir::dead_func_elim::dead_func_elim(&mut ir, &root_indices);
}
```

### Filetest path

DeadFuncElim is OFF by default in filetests (or roots = all functions).
Either way, no functions are removed.

### OptPass

Add `OptPass::DeadFuncElim` to the enum. Default: ON in production, OFF
in filetests.

## Dependencies

- **M0 (Stable CalleeRef):** Required so deletion doesn't break references.
- **M4 (Inliner wired in):** Without inlining, there are few dead functions
  to eliminate. DeadFuncElim is most useful after inlining has created dead
  functions.

## Validation

```bash
cargo test -p lpir
cargo test -p lps-filetests -- --test-threads=4
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf \
    --profile release-esp32 --features esp32c6,server
```

## Estimated scope

Small pass — ~50-100 lines. The hard part (stable ids) is in M0.
