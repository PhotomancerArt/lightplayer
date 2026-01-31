# Implementation Plan: Enable Fastalloc Register Allocator

## Goal

Enable Fastalloc (single-pass) register allocator instead of Ion (backtracking) to reduce memory
usage during compilation and prevent allocation failures on ESP32.

## Background

The Perlin function compilation is failing with:

```
memory allocation of 140 bytes failed
```

at `regalloc2::ion::Env::init`, indicating that Ion's initialization data structures require too
much memory. Fastalloc uses simpler data structures and should require less memory.

## Implementation Steps

### Step 1: Add Fastalloc Setting to ESP32 JIT Flags

**File:** `lp-glsl/esp32-glsl-jit/src/main.rs`

**Location:** Around line 230-234 where ISA flags are created

**Change:**

```rust
// Before:
let mut flag_builder = settings::builder();
flag_builder.set("opt_level", "none").unwrap();
flag_builder.set("is_pic", "false").unwrap();
flag_builder.set("enable_verifier", "false").unwrap();
let isa_flags = settings::Flags::new(flag_builder);

// After:
let mut flag_builder = settings::builder();
flag_builder.set("opt_level", "none").unwrap();
flag_builder.set("is_pic", "false").unwrap();
flag_builder.set("enable_verifier", "false").unwrap();
flag_builder.set("regalloc_algorithm", "single_pass").unwrap();  // ADD THIS LINE
let isa_flags = settings::Flags::new(flag_builder);
```

**Rationale:** This is the direct compilation path used by ESP32, so this change will immediately
affect the failing compilation.

### Step 2: Add Fastalloc Setting to Default RISC-V32 Flags

**File:** `lp-glsl/lp-glsl-compiler/src/backend/target/target.rs`

**Location:** `default_riscv32_flags()` function (around line 116-141)

**Change:**

```rust
fn default_riscv32_flags() -> Result<Flags, GlslError> {
    let mut flag_builder = settings::builder();
    flag_builder
        .set("is_pic", "true")
        .map_err(|e| GlslError::new(ErrorCode::E0400, format!("failed to set is_pic: {e}")))?;
    flag_builder
        .set("use_colocated_libcalls", "false")
        .map_err(|e| {
            GlslError::new(
                ErrorCode::E0400,
                format!("failed to set use_colocated_libcalls: {e}"),
            )
        })?;
    flag_builder
        .set("enable_multi_ret_implicit_sret", "true")
        .map_err(|e| {
            GlslError::new(
                ErrorCode::E0400,
                format!("failed to set enable_multi_ret_implicit_sret: {e}"),
            )
        })?;
    // ADD THIS BLOCK:
    flag_builder
        .set("regalloc_algorithm", "single_pass")
        .map_err(|e| {
            GlslError::new(
                ErrorCode::E0400,
                format!("failed to set regalloc_algorithm: {e}"),
            )
        })?;
    Ok(settings::Flags::new(flag_builder))
}
```

**Rationale:** This ensures all RISC-V32 emulator compilations use Fastalloc, providing consistent
behavior.

### Step 3: Add Fastalloc Setting to Default Host Flags (Optional)

**File:** `lp-glsl/lp-glsl-compiler/src/backend/target/target.rs`

**Location:** `default_host_flags()` function (around line 145-169)

**Change:** Similar to Step 2, add the `regalloc_algorithm` setting.

**Rationale:** For consistency, though host JIT compilation may not have memory constraints. This
can be optional if we want to keep better code quality on host.

**Decision:** Make this optional - only add if we want consistency, or leave it as backtracking for
better code quality on host systems.

### Step 4: Test Compilation

**Test Case:** Compile the failing Perlin function on ESP32

**Expected Result:**

- Compilation succeeds without memory allocation errors
- Generated code executes correctly
- Code quality may be slightly worse (more spills/moves) but acceptable

**Test Command:**

```bash
# Build and flash ESP32 firmware
cd lp-glsl/esp32-glsl-jit
cargo build --release --target riscv32imac-unknown-none-elf
# Flash and run on ESP32 hardware
```

### Step 5: Verify Code Quality (Optional)

**If needed:** Compare generated code between Ion and Fastalloc

**Metrics to check:**

- Number of register spills
- Number of register moves
- Code size
- Runtime performance

**Rationale:** Verify that Fastalloc produces acceptable code quality for embedded use case.

## Rollback Plan

If Fastalloc causes unacceptable code quality or other issues:

1. Remove the `regalloc_algorithm` setting lines added in Steps 1-3
2. This will revert to default `"backtracking"` (Ion)
3. Consider alternative solutions (increase heap size, optimize VCode size, etc.)

## Testing Checklist

- [ ] Compile Perlin function with Fastalloc - should succeed
- [ ] Run Perlin function on ESP32 - should execute correctly
- [ ] Verify no memory allocation errors during compilation
- [ ] (Optional) Compare code quality metrics vs Ion
- [ ] (Optional) Test other GLSL shaders to ensure no regressions

## Success Criteria

1. ✅ Perlin function compiles successfully without memory errors
2. ✅ Generated code executes correctly on ESP32
3. ✅ No regressions in other GLSL compilation paths
4. ✅ (Optional) Code quality is acceptable for embedded use

## Notes

- Fastalloc may produce slightly worse code quality (more spills/moves)
- For embedded targets, this trade-off is usually acceptable
- If code quality becomes an issue, we can investigate:
    - Increasing heap size as a workaround
    - Optimizing VCode size before register allocation
    - Other memory reduction strategies

## Related Files

- `lp-glsl/esp32-glsl-jit/src/main.rs` - ESP32 JIT compilation entry point
- `lp-glsl/lp-glsl-compiler/src/backend/target/target.rs` - Default flag configuration
-

`/Users/yona/dev/photomancer/feature/lp-cranelift-lp2025/cranelift/codegen/src/machinst/compile.rs` -
Register allocator selection logic
