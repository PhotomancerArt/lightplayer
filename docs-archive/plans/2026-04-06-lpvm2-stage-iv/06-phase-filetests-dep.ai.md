## Phase 6: Update lps-filetests to Use lpvm-emu

Temporarily update filetests to depend on `lpvm-emu` instead of `lpvm-cranelift` for RV32 emulation.

### Code Organization

**File: `lp-shader/lps-filetests/Cargo.toml`**

Change:
```toml
# Remove:
lpvm-cranelift = { path = "../lpvm-cranelift", features = ["riscv32-emu"] }

# Add:
lpvm-emu = { path = "../lpvm-emu" }
```

Also may need to add `lp-riscv-elf` and other deps that were transitive.

**File: `lp-shader/lps-filetests/src/test_run/lpir_rv32_executable.rs`**

Update imports:
```rust
// Change:
use lpvm_cranelift::{CompileOptions, CompilerError, glsl_q32_call_emulated, ...};

// To:
use lpvm_emu::{CompileOptions, EmuEngine, EmuModule};
use lpvm::{LpvmEngine, LpvmInstance, LpvmModule};
// Plus whatever else is needed for the old GlslExecutable trait
```

The `LpirRv32Executable` struct needs to be updated to either:
1. Wrap `EmuEngine` + `EmuModule` (but it needs to be mutable for calls)
2. Keep using the old API temporarily via re-exports from `lpvm-emu`
3. Implement a bridge

Since M5 will fully migrate filetests to LPVM traits, let's keep it simple:

**Option**: Add temporary re-exports from `lpvm-emu` for the old functions:

**File: `lp-shader/lpvm-emu/src/lib.rs` additions:**

```rust
// Temporary re-exports for lps-filetests compatibility during migration
#[cfg(feature = "std")]
pub mod compat {
    pub use lpvm_cranelift::{object_bytes_from_ir, link_object_with_builtins};
    pub use crate::engine::glsl_q32_call_emulated_compat;
}
```

Or simpler: Have `lps-filetests` directly use `lpvm-cranelift` for object/link (which it still can, those are separate), and only use `lpvm-emu` for the emulator parts.

Actually, let's reconsider: `object_bytes_from_ir` and `link_object_with_builtins` are still in `lpvm-cranelift` and don't need `lp-riscv-emu`. They just generate bytes. So `lps-filetests` can keep using `lpvm-cranelift` for that part, and only switch the actual emulation to `lpvm-emu` later.

But wait - we're removing those from `lpvm-cranelift` in this plan. They move to `lpvm-emu`.

Let's adjust: Keep `object_bytes_from_ir` and `link_object_with_builtins` in `lpvm-cranelift` but make them available to `lpvm-emu`. Actually, `lpvm-emu` needs them internally for compilation.

Revised approach:
1. `lpvm-emu` re-exports `object_bytes_from_ir` and `link_object_with_builtins` from its internal compile module
2. `lps-filetests` uses these re-exports
3. `lpvm-cranelift` removes the public exports but keeps internal functions if needed

**File: `lp-shader/lpvm-emu/src/compile.rs` additions:**

```rust
// Re-export compilation functions from lpvm-cranelift (kept there temporarily)
pub use lpvm_cranelift::object_bytes_from_ir;
pub use lpvm_cranelift::link_object_with_builtins;
```

Actually, if we're removing these from `lpvm-cranelift`, they need to move entirely. Let's have `lpvm-emu` own the RV32 object compilation path.

**Revised Phase 5+6:**

Don't remove `object_bytes_from_ir` and `link_object_with_builtins` from `lpvm-cranelift` - keep them but don't feature-gate them behind `riscv32-emu`. They're just pure codegen/linking, no emulator dependency.

Only move the actual `emu_run.rs` (execution) stuff to `lpvm-emu`.

### Revised approach

**Phase 5**: Only remove `lp-riscv-emu` dependency and `emu_run.rs`. Keep `object_bytes_from_ir`, `link_object_with_builtins`, `object_module.rs`, `object_link.rs` in `lpvm-cranelift`.

**Phase 6**: `lps-filetests` keeps using `lpvm-cranelift` for object/link. The `glsl_q32_call_emulated` function moves to `lpvm-emu`.

Update this phase accordingly.

### Validate

```bash
cargo check -p lps-filetests
cargo test -p lps-filetests --test rv32_tests  # or whatever the test file is called
```

Check that filetests still work with the new dependency structure.
