# Phase 6: CLI Integration

## Scope

Wire up `--show-region` and `--show-liveness` flags to display region tree and liveness analysis.

## Implementation

### 1. Update `lp-cli/src/commands/shader_rv32fa/args.rs`

Add new flags:

```rust
/// Show region tree structure on stderr.
#[arg(long, action = clap::ArgAction::SetTrue)]
pub show_region: bool,

/// Show liveness analysis on stderr.
#[arg(long, action = clap::ArgAction::SetTrue)]
pub show_liveness: bool,
```

Add to `Verbosity` struct:

```rust
pub struct Verbosity {
    pub vinst: bool,
    pub pinst: bool,
    pub disasm: bool,
    pub region: bool,
    pub liveness: bool,
}
```

### 2. Update pipeline or handler

After lowering, display region tree and liveness when requested:

```rust
let lowered = lower_ops(&func, &ir, &abi, float_mode)?;

if verbosity.region {
    let text = lpvm_native_fa::rv32::debug::region::format_region_tree(
        &lowered.region_tree,
        lowered.region_tree.root,
        &lowered.vinsts,
        &lowered.vreg_pool,
        &lowered.symbols,
        0,
    );
    eprintln!("=== Region Tree ===\n{}", text);
}

if verbosity.liveness {
    let liveness = lpvm_native_fa::alloc::liveness::analyze_liveness(
        &lowered.region_tree,
        lowered.region_tree.root,
        &lowered.vinsts,
        &lowered.vreg_pool,
    );
    eprintln!("{}", lpvm_native_fa::alloc::liveness::format_liveness(&liveness));
}
```

## Validate

```bash
cargo build -p lp-cli

# Test region tree display
./target/debug/lp-cli shader-rv32fa lp-shader/lps-filetests/filetests/debug/native-rv32-iadd.glsl --show-region

# Test liveness display
./target/debug/lp-cli shader-rv32fa lp-shader/lps-filetests/filetests/debug/native-rv32-iadd.glsl --show-liveness

# Test both
./target/debug/lp-cli shader-rv32fa lp-shader/lps-filetests/filetests/debug/native-rv32-iadd.glsl --show-region --show-liveness
```
