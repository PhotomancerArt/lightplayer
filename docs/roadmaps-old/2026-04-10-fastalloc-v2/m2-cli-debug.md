# M2: CLI and Debug Infrastructure

## Scope of Work

Create the `shader-rv32fa` CLI command with debug output flags, and wire up filetest integration for automatic trace display on failure.

## Files

```
lp-cli/src/commands/
├── mod.rs                    # UPDATE: add shader_rv32fa module
├── shader_rv32fa/
│   ├── mod.rs                # NEW: module exports
│   ├── args.rs               # NEW: command-line arguments
│   └── handler.rs            # NEW: command handler

lp-shader/lps-filetests/
└── src/
    └── runner.rs              # UPDATE: add trace capture on failure
```

## Implementation Details

### 1. Create `shader_rv32fa/args.rs`

```rust
//! Command-line arguments for shader-rv32fa command.

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "shader-rv32fa")]
#[command(about = "Compile GLSL to RV32 using fastalloc pipeline")]
pub struct Args {
    /// GLSL source file
    pub input: PathBuf,

    /// Output file (default: stdout)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Show LPIR
    #[arg(long)]
    pub show_lpir: bool,

    /// Show VInsts after lowering
    #[arg(long)]
    pub show_vinst: bool,

    /// Show CFG with liveness
    #[arg(long)]
    pub show_cfg: bool,

    /// Show PhysInsts after allocation
    #[arg(long)]
    pub show_physinst: bool,

    /// Show full allocation trace
    #[arg(long)]
    pub trace: bool,

    /// Emit assembly (not binary)
    #[arg(long)]
    pub emit_asm: bool,

    /// Output format
    #[arg(short, long, default_value = "bin")]
    pub format: OutputFormat,
}

#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
pub enum OutputFormat {
    #[default]
    Bin,
    Hex,
    Asm,
}
```

### 2. Create `shader_rv32fa/handler.rs`

```rust
//! Handler for shader-rv32fa command.

use std::fs;
use lpir::parse_glsl;
use lpvm_native::lower::lower_ops;
use lpvm_native::debug::vinst::format_vinsts;
use lpvm_native::isa::rv32fa::{
    alloc::FastAlloc,
    debug::physinst::format_physinsts,
    emit::emit,
};
use crate::commands::shader_rv32fa::args::Args;

pub fn handle(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    // Read and parse GLSL
    let source = fs::read_to_string(&args.input)?;
    let func = parse_glsl(&source)?;

    if args.show_lpir {
        println!("=== LPIR ===");
        println!("{}", func);
    }

    // Lower to VInst
    let vinsts = lower_ops(&func)?;

    if args.show_vinst {
        println!("=== VInst ===");
        println!("{}", format_vinsts(&vinsts));
    }

    // Build CFG (for debug output even in straight-line)
    let cfg = build_cfg(&vinsts);

    if args.show_cfg {
        println!("=== CFG ===");
        println!("{}", cfg);

        // Compute and show liveness
        let liveness = compute_liveness(&cfg);
        println!("=== Liveness ===");
        println!("{}", liveness);
    }

    // Allocate
    let result = match FastAlloc::allocate(&vinsts, func.vreg_types.len(), &func.abi) {
        Ok(r) => r,
        Err(e) => {
            // On error, show trace if available
            if let Some(trace) = e.trace() {
                eprintln!("=== Allocation failed at trace: ===");
                eprintln!("{}", trace);
            }
            return Err(e.into());
        }
    };

    if args.trace {
        println!("=== Allocation Trace ===");
        println!("{}", result.trace.format_table(&func.name));
    }

    if args.show_physinst {
        println!("=== PhysInst ===");
        println!("{}", format_physinsts(&result.physinsts));
    }

    // Emit
    let bytes = emit(&result.physinsts)?;

    // Output
    match args.format {
        OutputFormat::Bin => {
            if let Some(path) = args.output {
                fs::write(path, bytes)?;
            } else {
                use std::io::Write;
                std::io::stdout().write_all(&bytes)?;
            }
        }
        OutputFormat::Hex => {
            for byte in bytes {
                print!("{:02x}", byte);
            }
            println!();
        }
        OutputFormat::Asm => {
            // Disassemble output
            // ...
        }
    }

    Ok(())
}
```

### 3. Create `shader_rv32fa/mod.rs`

```rust
pub mod args;
pub mod handler;
```

### 4. Wire up in `lp-cli/src/commands/mod.rs`

```rust
pub mod shader_rv32fa;
```

### 5. Wire up in `lp-cli/src/main.rs`

```rust
Commands::ShaderRv32fa(args) => shader_rv32fa::handler::handle(args),
```

### 6. Update Filetest Runner

In `lp-shader/lps-filetests/src/runner.rs`, add trace capture when `DEBUG` env var is set:

```rust
fn run_filetest(path: &Path) -> Result<(), TestError> {
    // ... existing setup ...

    // Set allocator to Fast if filetest directives request it
    if directives.contains("fastalloc") {
        std::env::set_var("REG_ALLOC_ALGORITHM", "fast");
    }

    // Compile
    let result = std::panic::catch_unwind(|| {
        compile_shader(&source)
    });

    // If DEBUG is set and compilation failed, show trace
    if std::env::var("DEBUG").is_ok() {
        if let Err(ref e) = result {
            if let Some(trace) = extract_trace_from_error(e) {
                eprintln!("=== Filetest trace for {} ===", path.display());
                eprintln!("{}", trace);
            }
        }
    }

    // ... rest of assertions ...
}
```

### 7. Add Filetest Directives

Filetests can now use:

```glsl
// test: run
// allocator: fast
// debug: true

float test() {
    // ...
}
```

Or run with DEBUG env:

```bash
DEBUG=1 cargo test -p lps-filetests --test filetest -- some_test
```

## Tests

Test the CLI manually:

```bash
# Show all stages
cargo run -p lp-cli -- shader-rv32fa test.glsl \
    --show-vinst \
    --show-cfg \
    --show-physinst \
    --trace

# Just compile
cargo run -p lp-cli -- shader-rv32fa test.glsl -o test.bin

# Hex output
cargo run -p lp-cli -- shader-rv32fa test.glsl --format hex
```

## Validate

```bash
# Build
cargo build -p lp-cli

# Test CLI help works
cargo run -p lp-cli -- shader-rv32fa --help

# Test on a simple file
cargo run -p lp-cli -- shader-rv32fa \
    lp-shader/lps-filetests/filetests/scalar/int/op-add.glsl \
    --show-vinst
```

## Success Criteria

1. `shader-rv32fa` command exists and shows help
2. `--show-vinst` displays VInst text
3. `--show-physinst` attempts allocation and shows result (or stub)
4. `--trace` shows allocation trace
5. Filetest runner captures trace on failure when DEBUG=1
6. All debug output is useful and readable
