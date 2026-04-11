# M2 Plan Notes — Integration

## Current State

M1 complete. `fa_alloc::allocate` produces correct `Vec<PInst>` for
straight-line Linear/Seq regions. Unit tests pass.

Remaining code from pre-M1 era:
- `rv32::alloc` — old straight-line allocator
- `walk_region_stub`, `stub_entry`, `run_shell` — stub code from early development
- `emit.rs::emit_vinsts` — calls `rv32::alloc::allocate`
- `compile.rs::compile_function` — calls `rv32::alloc::allocate`
- `rv32/mod.rs::emit_function_fastalloc_bytes` — calls `rv32::alloc::allocate`
- `error.rs::NativeError::FastAlloc(rv32::alloc::AllocError)` — references old error type
- `lp-cli/src/commands/shader_rv32fa/pipeline.rs` — uses `rv32::alloc`

## Scope

### In scope
1. Replace `rv32::alloc` calls in `compile.rs` and `emit.rs` with `fa_alloc`
2. Delete `rv32/alloc.rs` and all old stub code
3. Update `error.rs` to use `fa_alloc::AllocError`
4. Update CLI pipeline to use `fa_alloc`
5. Add `Backend::Rv32fa` to `lps-filetests` target system
6. Wire `rv32fa` through `CompiledShader`, `FiletestInstance`, and `compile_glsl`
7. Add `lpvm-native-fa` dependency to `lps-filetests/Cargo.toml`
8. Validate straight-line filetests pass under `rv32fa`
9. Annotate control-flow/call filetests as `unimplemented: rv32fa.q32`

### Out of scope
- Control flow support (M3)
- Call support (M3)

## Q&A

### Q1: Should `rv32fa` reuse `NativeEmuEngine/Module/Instance` or get its own?

The `rt_emu` module in `lpvm-native-fa` already has `NativeEmuEngine`, etc.,
which internally calls `compile_module` → `compile_function` → `rv32::alloc`.
After we replace `rv32::alloc` with `fa_alloc` in `compile_function`, the
existing `NativeEmuEngine` automatically uses the new allocator. So **rv32fa
uses the same `NativeEmuEngine` types**—they just now call `fa_alloc` internally.

This means `rv32lp` (from `lpvm-native`) and `rv32fa` (from `lpvm-native-fa`)
are distinct backends using distinct crates, but the same type-level pattern.

**Answer**: `rv32fa` adds a parallel `CompiledShader::NativeFa` / `FiletestInstance::NativeFa`
variant, importing from `lpvm_native_fa`. This mirrors how `rv32lp` uses `lpvm_native`.

### Q2: Should `rv32fa` be in `DEFAULT_TARGETS`?

Not yet. Keep the default as `[rv32lp, rv32, wasm]`. Users can opt in with
`--target rv32fa`. We add it to `DEFAULT_TARGETS` once it's feature-complete (M3 done).

**Answer**: No, add to `ALL_TARGETS` only. Users opt in with `--target rv32fa`.

### Q3: What happens to `emit_function_fastalloc_bytes` in `rv32/mod.rs`?

It calls `rv32::alloc::allocate`. It should either be updated to use `fa_alloc`
or removed. Since it's a convenience function, we update it to use `fa_alloc`.

**Answer**: Update to use `fa_alloc::allocate`.

### Q4: How to handle `_func_abi` unused in `fa_alloc::allocate`?

The param precoloring is a known gap from M1. For now, `fa_alloc::allocate`
takes `FuncAbi` but doesn't use it for param fixups. This is fine for
straight-line filetests that don't have params. We'll address in M3 properly.

**Answer**: Keep as-is for M2. Param-heavy filetests will naturally be
annotated `unimplemented` if they fail.
