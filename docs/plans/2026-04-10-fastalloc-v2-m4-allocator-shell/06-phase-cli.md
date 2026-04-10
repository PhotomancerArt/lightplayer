# Phase 6: CLI Integration

## Scope

Wire up `--show-cfg` and `--show-liveness` flags to display CFG and liveness analysis.

## Implementation

### 1. Update `args.rs`

Add new flags to the `Args` struct:

```rust
/// Show CFG on stderr.
#[arg(long, action = clap::ArgAction::SetTrue)]
pub show_cfg: bool,

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
    pub cfg: bool,
    pub liveness: bool,
}

impl Args {
    pub fn verbosity(&self) -> Verbosity {
        let q = self.quiet;
        Verbosity {
            vinst: !q && !self.no_vinst,
            pinst: !q && !self.no_pinst,
            disasm: !q && !self.no_disasm,
            cfg: !q && self.show_cfg,
            liveness: !q && self.show_liveness,
        }
    }
}
```

### 2. Update `pipeline.rs`

Add CFG and liveness display to the pipeline:

```rust
use lpvm_native::isa::rv32fa::alloc::{cfg, liveness};

pub fn compile(
    glsl: &str,
    verbosity: Verbosity,
) -> Result<CompileResult, Error> {
    // ... existing lowering ...
    
    // Build CFG if requested
    if verbosity.cfg {
        let cfg = cfg::build_cfg(&lowered.vinsts);
        eprintln!("{}", cfg::format_cfg(&cfg));
    }
    
    // Build liveness if requested
    if verbosity.liveness {
        let cfg = cfg::build_cfg(&lowered.vinsts);
        let num_vregs = max_vreg_index(&lowered.vinsts);
        let live = liveness::analyze_liveness(&cfg, num_vregs);
        eprintln!("{}", liveness::format_liveness(&live));
    }
    
    // ... rest of compilation ...
}

fn max_vreg_index(vinsts: &[VInst]) -> usize {
    let mut max = 0;
    for v in vinsts {
        for u in v.uses() {
            max = max.max(u.0 as usize + 1);
        }
        for d in v.defs() {
            max = max.max(d.0 as usize + 1);
        }
    }
    max
}
```

### 3. Update `handler.rs`

Pass the new flags through to the pipeline.

## Validate

```bash
# Build CLI
cargo build -p lp-cli

# Test CFG display
./target/debug/lp-cli shader-rv32fa file.glsl --show-cfg

# Test liveness display
./target/debug/lp-cli shader-rv32fa file.glsl --show-liveness

# Test both
./target/debug/lp-cli shader-rv32fa file.glsl --show-cfg --show-liveness
```
