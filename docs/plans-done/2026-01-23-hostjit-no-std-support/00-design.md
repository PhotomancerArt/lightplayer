# Design: HostJit no_std and Embedded Support

## Overview

Expand `HostJit` target to support both std and no_std environments, enabling JIT compilation on embedded systems like ESP32. The main changes are:

1. **ISA Creation**: Support manual ISA creation in no_std mode (riscv32 only)
2. **Host Functions**: Use linker-resolved extern functions in no_std mode
3. **Flags**: Make flag creation work without std

## File Structure

```
lp-glsl-compiler/src/backend/target/
├── target.rs                    # MODIFY: Add create_host_isa() helper, update create_isa()
└── builder.rs                   # No changes needed

lp-glsl-compiler/src/backend/host/
├── mod.rs                       # MODIFY: Export extern declarations for no_std
├── registry.rs                  # MODIFY: Update get_host_function_pointer() for no_std
└── impls.rs                     # No changes (std-only implementations)

lp-glsl-compiler/src/backend/module/
└── gl_module.rs                 # MODIFY: Update symbol_lookup_fn to handle no_std extern functions
```

## Code Structure

### New Helper Function

**`create_host_isa(flags: Flags) -> Result<OwnedTargetIsa, GlslError>`**
- In std mode: uses `cranelift_native::builder()` for auto-detection
- In no_std mode: only supports riscv32, uses `riscv32::isa_builder(riscv32_triple())`
- Returns error in no_std if architecture is not riscv32

### Modified Functions

**`Target::create_isa()` for HostJit**
- Uses `create_host_isa()` helper instead of directly calling `cranelift_native`
- Falls back to cached ISA if already created

**`riscv32_triple()`**
- Currently gated behind `#[cfg(feature = "emulator")]`
- Make available without emulator feature (or duplicate for no_std use)

**`get_host_function_pointer(HostId) -> Option<*const u8>`**
- In std mode: returns pointers to implementations in `impls.rs` (current behavior)
- In no_std mode: returns pointers to extern functions `lp_jit_debug` and `lp_jit_print`

**`symbol_lookup_fn` in `GlModule::new_jit()`**
- In std mode: checks builtins, then host functions via `get_host_function_pointer()` (current)
- In no_std mode: checks builtins, then host functions via `get_host_function_pointer()` (now works)

### Extern Declarations

**New in `host/mod.rs` (no_std mode):**
```rust
#[cfg(not(feature = "std"))]
extern "C" {
    /// User-provided debug function (must be defined by user in no_std mode)
    fn lp_jit_debug(ptr: *const u8, len: usize);
    
    /// User-provided print function (must be defined by user in no_std mode)
    fn lp_jit_print(ptr: *const u8, len: usize);
}
```

### Flag Creation

**`default_host_flags()`**
- Keep as std-only convenience function (current behavior)
- Flags creation doesn't require std, but keeping it std-only for now is fine
- Users can create flags manually in no_std mode (as ESP32 app does)

## Implementation Details

### ISA Creation Flow

1. User creates `Target::HostJit` (manually or via constructor)
2. `create_isa()` is called (lazily, when needed)
3. If ISA already cached, return it
4. Otherwise, call `create_host_isa(flags)`
5. `create_host_isa()`:
   - If std: use `cranelift_native::builder()`
   - If no_std: use `riscv32::isa_builder(riscv32_triple())` (error if not riscv32)

### Host Function Resolution Flow

1. GLSL code calls `__host_debug` or `__host_println`
2. Symbol lookup via `symbol_lookup_fn` callback
3. `symbol_lookup_fn` calls `get_host_function_pointer(HostId)`
4. `get_host_function_pointer()`:
   - If std: returns pointer to `impls::__host_debug` or `impls::__host_println`
   - If no_std: returns pointer to `lp_jit_debug` or `lp_jit_print` extern functions
5. JIT code calls the resolved function pointer

### User Requirements (no_std)

Users must provide implementations for:
```rust
#[no_mangle]
pub extern "C" fn lp_jit_debug(ptr: *const u8, len: usize) {
    // User's debug implementation
}

#[no_mangle]
pub extern "C" fn lp_jit_print(ptr: *const u8, len: usize) {
    // User's print implementation
}
```

## Backward Compatibility

- All existing std-only code continues to work unchanged
- `host_jit()` constructor remains std-only (no change)
- `default_host_flags()` remains std-only (no change)
- Existing tests and examples continue to work

## Success Criteria

1. `Target::HostJit` can be used in no_std mode
2. `create_isa()` works for HostJit in no_std mode (riscv32 only)
3. Host functions resolve correctly in no_std mode via extern functions
4. ESP32 app can use HostJit without workarounds
5. All existing std-only code continues to work
6. No breaking changes to public API
