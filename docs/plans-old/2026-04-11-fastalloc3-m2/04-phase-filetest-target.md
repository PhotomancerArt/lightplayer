# Phase 4: Add `rv32fa` filetest target

## Goal

Add `Backend::Rv32fa` to the filetest system so tests can run against the new
allocator and compare results with cranelift.

## Changes

### `lps-filetests/Cargo.toml`
```toml
lpvm-native-fa = { path = "../lpvm-native-fa", features = ["emu"] }
```

### `lps-filetests/src/targets/mod.rs`

Add variant to `Backend`:
```rust
pub enum Backend {
    Jit,
    Rv32,
    Rv32lp,
    Rv32fa,  // NEW
    Wasm,
}
```

Add to `ALL_TARGETS`:
```rust
pub const ALL_TARGETS: &[Target] = &[
    // ... existing 4 ...
    Target {
        backend: Backend::Rv32fa,
        float_mode: FloatMode::Q32,
        isa: Isa::Riscv32,
        exec_mode: ExecMode::Emulator,
    },
];
```

Do NOT add to `DEFAULT_TARGETS` yet — users opt in via `--target rv32fa`.

### `lps-filetests/src/targets/display.rs`

Add `Display` arm for `Backend::Rv32fa`:
```rust
Backend::Rv32fa => write!(f, "rv32fa"),
```

Update error string in `parse_target_filters` to mention `rv32fa`.

### `lps-filetests/src/test_run/filetest_lpvm.rs`

Add imports:
```rust
use lpvm_native_fa::{
    NativeCompileOptions as FaCompileOptions,
    NativeEmuEngine as FaEmuEngine,
    NativeEmuInstance as FaEmuInstance,
    NativeEmuModule as FaEmuModule,
};
```

Add enum variants:
```rust
CompiledShader::NativeFa(FaEmuModule)
FiletestInstance::NativeFa(FaEmuInstance)
```

Add match arms in all `impl` methods, mirroring the `Native` (rv32lp) pattern.

### Test updates
- Update `test_default_targets_order_matches_const` if `ALL_TARGETS` indices shift
- Add `test_target_name_rv32fa_q32`

## Status: [ ]
