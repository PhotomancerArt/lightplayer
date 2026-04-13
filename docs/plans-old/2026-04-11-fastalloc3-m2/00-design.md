# M2 Design — Integration

## Overview

Wire `fa_alloc` into `compile_function`, delete `rv32::alloc`, add `rv32fa`
filetest target, validate straight-line filetests.

## Phase Plan

### Phase 1: Replace `rv32::alloc` with `fa_alloc` in compile pipeline

Files changed:
- `compile.rs` — replace `rv32::alloc::allocate(vinsts, func_abi, func, vreg_pool)` with
  `fa_alloc::allocate(&lowered, &func_abi)`, extract `pinsts` from `AllocResult`
- `emit.rs` — same replacement in `emit_vinsts`
- `rv32/mod.rs` — update `emit_function_fastalloc_bytes` to use `fa_alloc`

The key change is that `fa_alloc::allocate` takes a `&LoweredFunction` (which
contains vinsts, vreg_pool, region_tree), while the old `rv32::alloc::allocate`
took individual `&[VInst]`, `&FuncAbi`, `&IrFunction`, `&[VReg]`.

`compile_function` already produces a `LoweredFunction` — we just pass it
directly to `fa_alloc::allocate` instead of destructuring.

### Phase 2: Delete old code

Files changed:
- Delete `rv32/alloc.rs`
- `rv32/mod.rs` — remove `pub mod alloc;`
- `error.rs` — change `FastAlloc(crate::rv32::alloc::AllocError)` to
  `FastAlloc(crate::fa_alloc::AllocError)`
- `fa_alloc/mod.rs` — remove `run_shell`, `walk_region_stub` usage
- `fa_alloc/walk.rs` — remove `walk_region_stub` function
- `fa_alloc/trace.rs` — remove `stub_entry`, `stub_detail` helpers
- Update any tests that reference old code

### Phase 3: Update CLI pipeline

Files changed:
- `lp-cli/src/commands/shader_rv32fa/pipeline.rs` — replace `rv32::alloc`
  import with `fa_alloc::allocate`, adjust call site

### Phase 4: Add `rv32fa` filetest target

Files changed:
- `lps-filetests/Cargo.toml` — add `lpvm-native-fa = { path = "../lpvm-native-fa", features = ["emu"] }`
- `lps-filetests/src/targets/mod.rs` — add `Backend::Rv32fa`, add to `ALL_TARGETS`
- `lps-filetests/src/targets/display.rs` — add `Display` arm, update error string
- `lps-filetests/src/test_run/filetest_lpvm.rs`:
  - Add `use lpvm_native_fa::{NativeCompileOptions as FaCompileOptions, NativeEmuEngine as FaEmuEngine, NativeEmuInstance as FaEmuInstance, NativeEmuModule as FaEmuModule};`
  - Add `CompiledShader::NativeFa(FaEmuModule)` variant
  - Add `FiletestInstance::NativeFa(FaEmuInstance)` variant
  - Add match arms in `compile_glsl`, `instantiate`, `call`, `call_q32_flat`,
    `debug_state`, `last_guest_instruction_count`, `module_sig`

### Phase 5: Filetest validation + annotations

- Run filetests with `--target rv32fa`
- Identify failures (control flow, calls, params)
- Add `// unimplemented: rv32fa.q32` annotations to failing filetests
- Confirm straight-line filetests match cranelift results

### Phase 6: Cleanup

- Fix warnings
- Run full test suite
- Confirm `cargo check` passes for all relevant targets

## Validation

```bash
cargo check -p lpvm-native-fa
cargo test -p lpvm-native-fa
cargo check -p lps-filetests
cargo test -p lps-filetests -- rv32fa
# Run filetests with rv32fa target
cargo test -p lps-filetests --test filetests -- --target rv32fa
```
