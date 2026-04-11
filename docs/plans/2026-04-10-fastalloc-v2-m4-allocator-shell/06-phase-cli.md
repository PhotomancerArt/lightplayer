# Phase 6: CLI Integration

## Scope

Wire up `--show-region` and `--show-liveness` flags to display region tree and liveness analysis.

## Implementation

### 1. Update `args.rs`

Add new flags to the `Args` struct:

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
    pub region: bool,       // Changed from cfg
    pub liveness: bool,
}

impl Args {
    pub fn verbosity(&self) -> Verbosity {
        let q = self.quiet;
        Verbosity {
            vinst: !q && !self.no_vinst,
            pinst: !q && !self.no_pinst,
            disasm: !q && !self.no_disasm,
            region: !q && self.show_region,      // Changed from cfg
            liveness: !q && self.show_liveness,
        }
    }
}
```

### 2. Update `pipeline.rs`

Add region and liveness display to the pipeline:

```rust
use lpvm_native::isa::rv32fa::alloc::liveness;
use lpvm_native::debug::region;

pub fn compile(
    glsl: &str,
    verbosity: Verbosity,
) -> Result<CompileResult, Error> {
    // ... existing lowering ...
    let lowered = lower_ops(&func, &ir, &abi, float_mode)?;
    
    // Display region tree if requested
    if verbosity.region {
        let region_text = region::format_region(&lowered.region, &lowered.vinsts, 0);
        eprintln!("{}", region_text);
    }
    
    // Display liveness if requested
    if verbosity.liveness {
        let liveness = liveness::analyze_liveness(&lowered.region, &lowered.vinsts);
        eprintln!("{}", liveness::format_liveness(&liveness));
    }
    
    // ... rest of compilation ...
}
```

### 3. Update `handler.rs`

Pass the new flags through to the pipeline.

## Validate

```bash
# Build CLI
cargo build -p lp-cli

# Test region tree display
./target/debug/lp-cli shader-rv32fa file.glsl --show-region

# Test liveness display
./target/debug/lp-cli shader-rv32fa file.glsl --show-liveness

# Test both
./target/debug/lp-cli shader-rv32fa file.glsl --show-region --show-liveness
```
