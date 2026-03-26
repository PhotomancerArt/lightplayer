# Stage VI-A: lpir-cranelift embedded readiness — design

## Scope

Make `lpir-cranelift` compile and run correctly without `std` (targeting
`riscv32imac-unknown-none-elf`) and expand `CompileOptions` with Q32 arithmetic
modes, memory strategy, and error bounds. Validation: `cargo check` cross-compile.
Functional validation deferred to VI-B (`fw-emu`).

## File structure

```
lpir-cranelift/
├── Cargo.toml                    # UPDATE: std (default), cranelift-optimizer, cranelift-verifier features
├── build.rs                      # UNCHANGED (build scripts are host-only)
└── src/
    ├── lib.rs                    # UPDATE: #![no_std], conditional extern crate std
    ├── compile.rs                # UNCHANGED (all entry points available everywhere)
    ├── compile_options.rs        # UPDATE: add q32_options, memory_strategy, max_errors
    ├── q32_options.rs            # NEW: Q32Options, AddSubMode, MulMode, DivMode
    ├── jit_module.rs             # UPDATE: ISA selection — cranelift-native behind std, explicit ISA path
    ├── module_lower.rs           # UPDATE: LowMemory metadata strip after define_function
    ├── process_sync.rs           # UPDATE: #[cfg(feature = "std")] real mutex; else no-op guard
    ├── error.rs                  # UPDATE: gate std::error::Error impls behind std
    ├── values.rs                 # UPDATE: gate std::error::Error impl behind std
    ├── builtins.rs               # UNCHANGED
    ├── emit/                     # UNCHANGED
    ├── call.rs                   # UNCHANGED
    ├── invoke.rs                 # UNCHANGED
    ├── direct_call.rs            # UNCHANGED
    ├── emu_run.rs                # UNCHANGED (behind riscv32-emu → std)
    ├── object_link.rs            # UNCHANGED (behind riscv32-emu)
    └── object_module.rs          # UNCHANGED (behind riscv32-emu)
```

## Cargo feature layout

```toml
[features]
default = ["std"]
std = [
    "cranelift-codegen/std",
    "cranelift-codegen/host-arch",
    "cranelift-frontend/std",
    "cranelift-module/std",
    "cranelift-jit/std",
    "cranelift-native",
    "lp-glsl-builtins/std",
]
cranelift-optimizer = ["cranelift-codegen/optimizer"]
cranelift-verifier = ["cranelift-codegen/verifier"]
riscv32-emu = [
    "std",
    "dep:cranelift-object",
    "dep:lp-riscv-elf",
    "dep:lp-riscv-emu",
    "cranelift-codegen/riscv32",
]
```

Without `std`: `no_std` + `alloc`. `cranelift-native` absent (ISA must be
provided explicitly). `process_sync` is a no-op. `riscv32-emu` implies `std`.

## CompileOptions

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompileOptions {
    pub float_mode: FloatMode,
    pub q32_options: Q32Options,
    pub memory_strategy: MemoryStrategy,
    pub max_errors: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MemoryStrategy {
    #[default]
    Default,
    LowMemory,
}
```

Default: `float_mode = Q32`, `q32_options = Q32Options::default()` (all
saturating), `memory_strategy = Default`, `max_errors = None`.

## Q32Options (new file: `q32_options.rs`)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Q32Options {
    pub add_sub: AddSubMode,
    pub mul: MulMode,
    pub div: DivMode,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AddSubMode { #[default] Saturating, Wrapping }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MulMode { #[default] Saturating, Wrapping }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DivMode { #[default] Saturating, Reciprocal }
```

Mirror of `lp-model::GlslOpts` enums but compiler-internal. `lp-engine` maps
`GlslOpts → Q32Options` (VI-B scope).

Note: `Q32Options` is plumbed into `CompileOptions` and stored, but the emitter
does not yet branch on mode variants — it currently emits saturating builtins
unconditionally. Wiring the mode to different builtin selection is a follow-up
when wrapping/reciprocal builtins exist.

## ISA selection

In `jit_module::build_jit_module`:

```rust
#[cfg(feature = "std")]
fn build_isa(flags: settings::Flags) -> Result<OwnedTargetIsa, CompilerError> {
    cranelift_native::builder()
        .map_err(|m| ...)?
        .finish(flags)
        .map_err(|e| ...)
}

#[cfg(not(feature = "std"))]
fn build_isa(flags: settings::Flags) -> Result<OwnedTargetIsa, CompilerError> {
    use cranelift_codegen::isa;
    use target_lexicon::Triple;

    // riscv32imac for ESP32-C6; could be parameterized later
    let triple: Triple = "riscv32imac-unknown-none-elf".parse()
        .map_err(|e| ...)?;
    isa::lookup(triple)
        .map_err(|e| ...)?
        .finish(flags)
        .map_err(|e| ...)
}
```

Future: `CompileOptions` may carry an optional `Triple` or ISA builder to make
this fully dynamic. For now, hard-coded `riscv32imac` on `no_std` matches the
only embedded target.

## process_sync

```rust
#[cfg(feature = "std")]
mod imp {
    use std::sync::{Mutex, MutexGuard, OnceLock};
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    pub(crate) fn codegen_guard() -> MutexGuard<'static, ()> {
        LOCK.get_or_init(|| Mutex::new(())).lock().expect("mutex poisoned")
    }
}

#[cfg(not(feature = "std"))]
mod imp {
    pub(crate) struct NoopGuard;
    impl Drop for NoopGuard { fn drop(&mut self) {} }
    pub(crate) fn codegen_guard() -> NoopGuard { NoopGuard }
}

pub(crate) use imp::codegen_guard;
```

## LowMemory in module_lower

After `module.define_function(fid, &mut ctx)`, when
`options.memory_strategy == LowMemory`:

- Call `ctx.clear()` (already done).
- Drop `func_ctx` (already scoped).
- The `IrFunction` is borrowed from `ir: &IrModule` — cannot be dropped here.
  For owned paths (`jit_from_ir_owned`), the caller can drain functions from
  `IrModule` between iterations. This is the same pattern as the old crate's
  streaming pipeline.

Additional: investigate whether `module.clear_context(&mut ctx)` (Cranelift
module-level clear) exists and frees more internal state than `ctx.clear()`.

## std::error::Error gating

Gate `impl std::error::Error` behind `#[cfg(feature = "std")]` in:
- `error.rs`: `CompileError`, `CompilerError`
- `values.rs`: `CallError`

## Validation

```bash
# Cross-compile check (no std leaks)
cargo check --target riscv32imac-unknown-none-elf -p lpir-cranelift --no-default-features

# Host tests still pass (std + riscv32-emu)
cargo test -p lpir-cranelift --features riscv32-emu

# Filetests still pass
just glsl-filetests
```
