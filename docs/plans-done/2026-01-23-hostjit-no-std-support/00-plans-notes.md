# Plans Notes: HostJit no_std and Embedded Support

## Current State

### HostJit Implementation
- `Target::HostJit` enum variant exists and can be manually constructed
- `host_jit()` constructor requires `#[cfg(feature = "std")]` 
- `default_host_flags()` requires `#[cfg(feature = "std")]`
- `create_isa()` for HostJit uses `cranelift_native::builder()` which requires std
- When std is unavailable, `create_isa()` returns error: "std feature required for host JIT"
- **Note**: `cranelift_native` is just a convenience wrapper for auto-detecting host architecture
- We don't actually need it - can use architecture-specific builders directly (e.g., `riscv32::isa_builder`)

### JITModule Support
- `cranelift-jit::JITModule` supports no_std (confirmed in Cargo.toml comment)
- `GlslJitModule` comment states: "Works in both std and no_std (JITModule supports no_std)"
- The JIT execution path itself doesn't require std

### ESP32 Workaround
- ESP32 app manually creates ISA using `cranelift_codegen::isa::riscv32::isa_builder(triple)`
- Manually constructs `Target::HostJit { arch: None, flags, isa: None }`
- Still fails when `create_isa()` is called because it tries to use `cranelift_native`

### Host Functions
- `HostId` enum in `lp-glsl-compiler/src/backend/host/registry.rs` identifies host functions (Debug, Println)
- `get_host_function_pointer()` returns `None` in no_std mode (line 62-63)
- Implementations in `impls.rs` use `std::println!` which requires std
- Host functions are registered via `symbol_lookup_fn` in `gl_module.rs`

## Questions

### Q1: Target Naming Strategy ✅ ANSWERED
**Question**: Should we rename `HostJit` to `StdJit` and create a separate `EmbeddedJit`/`NoStdJit`, or expand `HostJit` to support both std and no_std modes?

**Context**: 
- "HostJit" is semantic (runs on current machine) and doesn't inherently imply std requirement
- Current implementation uses `cranelift_native` for convenience (auto-detects host architecture)
- `cranelift_native` requires std, but we don't actually need it - can use architecture-specific builders directly
- ESP32 app shows JIT can work without std by manually creating ISA using `riscv32::isa_builder(triple)`

**Answer**: Expand `HostJit` to support both modes:
- Keep the semantic name (runs on current machine)
- Support std mode (auto-detect via `cranelift_native`) and no_std mode (manual ISA creation)
- Less breaking changes, single target type for "JIT on current machine"

### Q2: ISA Creation Strategy ✅ ANSWERED
**Question**: How should `create_isa()` handle no_std mode for HostJit?

**Context**:
- With std: use `cranelift_native::builder()` for convenience (auto-detects host architecture)
- Without std: use manual ISA creation using architecture-specific builders
- ESP32 uses `cranelift_codegen::isa::riscv32::isa_builder(triple)` - this works fine without std
- `HostJit` has optional `arch: Option<Architecture>` field (currently unused)
- Architecture-specific builders (riscv32, x86_64, etc.) don't require std
- **User preference**: Write a simple function that in std uses `cranelift_native`, and without std only supports riscv32 (error otherwise)

**Answer**: 
- Create helper function `create_host_isa(flags: Flags) -> Result<OwnedTargetIsa, GlslError>`
- In std mode: use `cranelift_native::builder()` (current behavior)
- In no_std mode: only support riscv32, use `riscv32::isa_builder(riscv32_triple())`, error for other architectures
- Use this helper in `create_isa()` for HostJit
- Keep `riscv32_triple()` helper (already exists, just needs to be available without emulator feature)

### Q3: Host Function Implementation Strategy ✅ ANSWERED
**Question**: How should host functions (Debug, Println) be handled in no_std mode?

**Context**:
- Current `get_host_function_pointer()` returns `None` in no_std mode
- Implementations use `std::println!` which requires std
- User mentioned requiring linker-provided implementation for no_std
- Host functions are registered via `symbol_lookup_fn` callback
- User wants to use `extern lp_jit_print` and `lp_jit_debug` that must be defined by the user

**Answer**:
- Keep symbol names as `__host_debug` and `__host_println` (what GLSL code calls)
- In std mode: resolve to our implementations in `impls.rs`
- In no_std mode: declare extern functions `extern "C" { fn lp_jit_print(...); fn lp_jit_debug(...); }`
- Get function pointers via `lp_jit_print as *const u8` (linker resolves the symbols)
- Map `__host_debug` → `lp_jit_debug` and `__host_println` → `lp_jit_print` in symbol_lookup_fn for no_std
- Document that no_std users must provide implementations for `lp_jit_debug` and `lp_jit_print` extern functions
- The extern functions should match the signature: `extern "C" fn(ptr: *const u8, len: usize)`

### Q4: Constructor API Design
**Question**: What constructors should be available for creating HostJit targets?

**Context**:
- Current: `host_jit()` requires std
- ESP32 needs: manual ISA creation path
- Need to support both std and no_std users

**Suggested Answer**:
- `host_jit()` - std only, auto-detect architecture
- `host_jit_with_arch(arch: Architecture)` - std only, specific architecture
- `host_jit_with_isa(isa: OwnedTargetIsa, flags: Flags)` - no_std, user provides ISA
- `host_jit_manual(arch: Architecture, flags: Flags)` - no_std, creates ISA manually

### Q5: Default Flags Strategy
**Question**: How should `default_host_flags()` work in no_std mode?

**Context**:
- Current `default_host_flags()` requires std
- Flags are just settings, don't inherently require std
- ESP32 app manually creates flags

**Suggested Answer**:
- Move flag creation logic to a no_std compatible function
- `default_host_flags()` can remain std-only for convenience
- Add `default_host_flags_no_std()` or make it work without std feature gate
- Flags are just `settings::Flags`, no std requirement

### Q6: Backward Compatibility
**Question**: How do we ensure existing std-only code continues to work?

**Context**:
- Many tests and examples use `host_jit()`
- Need to ensure no breaking changes for std users

**Suggested Answer**:
- Keep all existing std constructors unchanged
- Add new no_std constructors alongside
- Ensure `create_isa()` falls back gracefully when std unavailable but ISA already provided

### Q7: Host Function Registration API ✅ ANSWERED (via Q3)
**Question**: What API should be provided for no_std users to register host function implementations?

**Context**:
- Need a way for no_std users to provide Debug/Println implementations
- Could be at Target creation time, Module creation time, or global registry
- Should be type-safe and easy to use

**Answer**: 
- No registration API needed - use extern functions that linker resolves
- User provides `extern "C" fn lp_jit_print(...)` and `extern "C" fn lp_jit_debug(...)` in their code
- We get function pointers via `lp_jit_print as *const u8` (linker resolves)
- Simpler than registration API - just requires user to define the extern functions

## Notes

- User requirement: Host functions (Debug, Println) should require linker-provided implementation in no_std mode
- Current ESP32 app shows the pattern: manually create ISA, manually construct Target
- JITModule itself supports no_std, so the main blockers are ISA creation and host function linking
