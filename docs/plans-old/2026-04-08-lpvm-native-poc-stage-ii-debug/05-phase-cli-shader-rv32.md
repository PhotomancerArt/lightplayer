# Phase 5: lp-cli shader-rv32 Command

## Scope

Add `lp-cli shader-rv32` command to compile GLSL shader to annotated RV32 assembly.

## Code Organization Reminders

- Follow existing `shader_lpir` pattern in `lp-cli/src/commands/`
- Use `clap` for argument parsing
- Compile with `debug_info: true` to get annotations
- Output to stdout by default, file with `--output`

## Implementation Details

### Create `lp-cli/src/commands/shader_rv32/mod.rs`

```rust
//! `shader-rv32` subcommand - compile GLSL to annotated RV32 assembly.

pub mod args;
pub mod handler;

pub use args::ShaderRv32Args;
pub use handler::handle;
```

### Create `lp-cli/src/commands/shader_rv32/args.rs`

```rust
//! Arguments for `shader-rv32` subcommand.

use std::path::PathBuf;
use clap::Parser;

/// Compile a GLSL shader to annotated RV32 assembly.
#[derive(Parser, Debug)]
pub struct ShaderRv32Args {
    /// Path to GLSL shader file
    pub file: PathBuf,

    /// Output file (default: stdout)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Float mode (q32 or f32)
    #[arg(long, default_value = "q32")]
    pub float_mode: String,

    /// Include raw hex offsets in comments
    #[arg(long)]
    pub hex: bool,
}
```

### Create `lp-cli/src/commands/shader_rv32/handler.rs`

```rust
//! Handler for `shader-rv32` subcommand.

use std::fs;
use anyhow::{Context, Result};
use lps_frontend;
use lpir::FloatMode;
use lpvm_native::{
    NativeCompileOptions, NativeEngine,
    isa::rv32::debug::{disassemble_function, LineTable},
};
use lps_shared::LpsModuleSig;
use lpvm::LpvmEngine;

use super::args::ShaderRv32Args;

pub fn handle(args: ShaderRv32Args) -> Result<()> {
    // Read GLSL source
    let source = fs::read_to_string(&args.file)
        .with_context(|| format!("Failed to read {}", args.file.display()))?;

    // Parse GLSL
    let naga = lps_frontend::compile(&source)
        .map_err(|e| anyhow::anyhow!("GLSL parse error: {}", e))?;

    // Lower to LPIR
    let (ir, meta) = lps_frontend::lower(&naga)
        .map_err(|e| anyhow::anyhow!("LPIR lowering error: {}", e))?;

    // Determine float mode
    let float_mode = match args.float_mode.as_str() {
        "q32" => FloatMode::Q32,
        "f32" => FloatMode::F32,
        _ => anyhow::bail!("Invalid float mode: {} (use q32 or f32)", args.float_mode),
    };

    // Compile with debug info enabled
    let options = NativeCompileOptions {
        float_mode,
        debug_info: true,  // Always enable for assembly output
    };
    let engine = NativeEngine::new(options);

    // Get assembly output from engine
    let asm_output = engine.compile_to_asm(&ir, &meta)
        .map_err(|e| anyhow::anyhow!("Compilation error: {:?}", e))?;

    // Write output
    if let Some(out_path) = args.output {
        fs::write(&out_path, asm_output)
            .with_context(|| format!("Failed to write {}", out_path.display()))?;
        println!("Wrote assembly to {}", out_path.display());
    } else {
        println!("{}", asm_output);
    }

    Ok(())
}
```

### Alternative: Direct approach without new Engine API

If we don't want to add `compile_to_asm()` to `NativeEngine`, we can do the steps inline:

```rust
// In handler.rs, alternative implementation:

// Lower and emit
let vinsts = lpvm_native::lower::lower_function(&ir.functions[0], options.float_mode)
    .map_err(|e| anyhow::anyhow!("Lowering error: {:?}", e))?;

// Allocate
let alloc = lpvm_native::regalloc::GreedyAlloc::new()
    .allocate(&ir.functions[0], &vinsts)
    .map_err(|e| anyhow::anyhow!("Register allocation error: {:?}", e))?;

// Emit with debug tracking
let mut ctx = lpvm_native::isa::rv32::EmitContext::new_with_debug(/* ... */);
ctx.emit_function(&ir.functions[0], &vinsts, &alloc)
    .map_err(|e| anyhow::anyhow!("Emission error: {:?}", e))?;

// Create line table
let line_table = LineTable::from_pairs(&ctx.debug_lines);

// Disassemble
let asm = disassemble_function(&ctx.code, &line_table, &ir, &ir.functions[0]);
```

### Update `lp-cli/src/commands/mod.rs`

```rust
pub mod shader_lpir;
pub mod shader_rv32;  // NEW
```

### Update `lp-cli/src/main.rs`

Add to `Cli` enum:

```rust
#[derive(Subcommand)]
enum Commands {
    /// Run the server
    Serve(ServeArgs),
    /// Create a new project
    New(NewArgs),
    /// Build a project
    Build(BuildArgs),
    /// Dump LPIR for a GLSL shader
    ShaderLpir(super::commands::shader_lpir::ShaderLpirArgs),
    /// Compile GLSL to annotated RV32 assembly  // NEW
    ShaderRv32(super::commands::shader_rv32::ShaderRv32Args),
}
```

Add to `match` in `run()`:

```rust
Commands::ShaderRv32(args) => {
    commands::shader_rv32::handle(args)?;
}
```

## Tests

```bash
# Compile simple shader to assembly
cargo run -p lp-cli -- shader-rv32 filetests/scalar/int/op-add.glsl

# Check output contains expected patterns
cargo run -p lp-cli -- shader-rv32 filetests/scalar/int/op-add.glsl | grep -q ".globl"
cargo run -p lp-cli -- shader-rv32 filetests/scalar/int/op-add.glsl | grep -q "LPIR"

# Write to file
cargo run -p lp-cli -- shader-rv32 filetests/scalar/int/op-add.glsl --output /tmp/out.s
cat /tmp/out.s
```

## Validate

```bash
cargo check -p lp-cli
cargo test -p lp-cli --bin lp-cli -- --test-threads=1 shader_rv32
```

## Dependencies

May need to add to `lp-cli/Cargo.toml`:

```toml
[dependencies]
# ... existing ...
lpvm-native = { path = "../lp-shader/lpvm-native" }
```

And ensure `lpvm-native` has appropriate feature gating so it compiles on host.
