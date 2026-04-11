# JIT Compilation Issue: is_pic Configuration Mismatch

## ✅ RESOLVED

This issue has been fixed. `is_pic` is now correctly set to:

- `true` for emulator object linking mode (`default_riscv32_flags()`)
- `false` for JIT mode (`default_host_flags()`)

See `lightplayer/crates/lps-compiler/src/backend/target/target.rs`:

- Line 134: `is_pic="true"` for RISC-V emulator
- Line 162: `is_pic="false"` for HostJit

## Problem

When compiling GLSL shaders using `lps-compiler`'s `glsl_jit()` function, the compilation fails with:

```
cranelift-jit needs is_pic=false
```

This panic occurs in `cranelift/jit/src/backend.rs:411` when creating a `JITModule`.

## Root Cause (Historical)

**Configuration mismatch between `lps-compiler` and `cranelift-jit` (now fixed):**

1. **`lps-compiler`** was incorrectly setting `is_pic=true` in `default_host_flags()`:

   - File: `lightplayer/crates/lps-compiler/src/backend/target/target.rs:163` (now fixed)
   - Previously: `.set("is_pic", "true")`
   - Now: `.set("is_pic", "false")` with comment "Disable PIC for JIT target - cranelift-jit requires is_pic=false"

2. **`cranelift-jit`** requires `is_pic=false`:
   - File: `cranelift/jit/src/backend.rs:411-413`
   - Code: `assert!(!builder.isa.flags().is_pic(), "cranelift-jit needs is_pic=false");`

## Where It Fails

- **Test**: `lp-core/src/nodes/shader/runtime.rs::test_shader_node_runtime_init_valid`
- **Code path**: `ShaderNodeRuntime::init()` → `glsl_jit()` → `compile_glsl_to_gl_module_jit()` → `GlModule::build_executable()` → `JITModule::new()`

## Impact

- Shader compilation in `lp-core` cannot work with current `lps-compiler` configuration
- This blocks Phase 6 (Shader Node Runtime) completion
- Tests that compile shaders will fail

## Possible Solutions

1. **Change `lps-compiler` to use `is_pic=false` for HostJit mode**:

   - Modify `default_host_flags()` to set `is_pic=false` when `run_mode == HostJit`
   - Keep `is_pic=true` for emulator mode (RISC-V)

2. **Add a flag/option to `GlslOptions`**:

   - Allow caller to specify PIC setting
   - Default to `false` for HostJit, `true` for Emulator

3. **Check if `lps-compiler` actually needs PIC for HostJit**:
   - Investigate why PIC was enabled for host target
   - May have been copy-pasted from emulator config

## Investigation Needed

- Why was `is_pic=true` set for host target in `lps-compiler`?
- Does HostJit actually need PIC, or was this a mistake?
- Can we conditionally set PIC based on run mode?

## Workaround for Now

- Skip shader compilation tests that require JIT
- Mark test as `#[ignore]` with a note about the JIT issue
- Continue with other phases that don't require shader compilation
