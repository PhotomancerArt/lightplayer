# Phase 4: Create shader-debug CLI Command

## Scope

Create the new `shader-debug` CLI command that uses `ModuleDebugInfo` from any backend and prints unified debug output. Remove the old `shader-rv32fa` and `shader-rv32` commands.

## Implementation Details

### 1. Create `lp-cli/src/commands/shader_debug/args.rs`

```rust
//! Arguments for `shader-debug`.

use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(about = "Debug shader compilation: show LPIR, VInsts, allocations, and disassembly")]
pub struct Args {
    /// Path to GLSL source file
    pub input: PathBuf,

    /// Target backend to compile with (e.g., rv32fa, rv32, rv32lp)
    #[arg(short, long)]
    pub target: String,

    /// Show only a specific function (default: show all)
    #[arg(long)]
    pub fn_name: Option<String>,

    /// Floating point mode
    #[arg(long, default_value = "q32")]
    pub float_mode: String,
}
```

### 2. Create `lp-cli/src/commands/shader_debug/handler.rs`

```rust
//! Handler for `shader-debug` command.

use anyhow::{Context, Result};
use lpir::FloatMode;

use crate::commands::shader_debug::args::Args;

pub fn handle(args: Args) -> Result<()> {
    let src = std::fs::read_to_string(&args.input)
        .with_context(|| format!("read {}", args.input.display()))?;

    let naga = lps_frontend::compile(&src).context("GLSL parse (Naga)")?;
    let (ir, sig) = lps_frontend::lower(&naga).context("lower to LPIR")?;

    let float_mode = match args.float_mode.as_str() {
        "q32" => FloatMode::Q32,
        "f32" => FloatMode::F32,
        _ => anyhow::bail!("invalid --float-mode (use q32 or f32)"),
    };

    // Compile with the specified backend and get debug info
    let debug_info = compile_and_get_debug(&args.target, &ir, &sig, float_mode)?;

    // Render output
    let output = if let Some(filter) = &args.fn_name {
        if !debug_info.functions.contains_key(filter) {
            anyhow::bail!("Function '{}' not found. Available: {}",
                filter,
                debug_info.function_names().join(", ")
            );
        }
        debug_info.render(Some(filter.as_str()))
    } else {
        debug_info.render(None)
    };

    print!("{}", output);

    // Print help text with copy-pasteable examples
    let help = debug_info.help_text(
        &args.input.display().to_string(),
        &args.target
    );
    println!("{}", help);

    Ok(())
}

fn compile_and_get_debug(
    target: &str,
    ir: &lpir::LpirModule,
    sig: &lps_frontend::LpsModuleSig,
    float_mode: FloatMode,
) -> Result<lpvm::ModuleDebugInfo> {
    use lpvm::LpvmModule;

    match target {
        "rv32fa" | "rv32fa.q32" => {
            use lpvm_native::{NativeCompileOptions, NativeEmuEngine};
            let opts = NativeCompileOptions {
                float_mode,
                ..Default::default()
            };
            let engine = NativeEmuEngine::new(opts);
            let module = engine.compile(ir, sig)
                .map_err(|e| anyhow::anyhow!("FA compile failed: {e}"))?;
            module.debug_info()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("FA backend did not produce debug info"))
        }
        "rv32" | "rv32.q32" => {
            use lpvm_native::{NativeCompileOptions, NativeEmuEngine};
            let opts = NativeCompileOptions {
                float_mode,
                ..Default::default()
            };
            let engine = NativeEmuEngine::new(opts);
            let module = engine.compile(ir, sig)
                .map_err(|e| anyhow::anyhow!("Native compile failed: {e}"))?;
            module.debug_info()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Native backend did not produce debug info"))
        }
        "rv32lp" | "rv32lp.q32" => {
            use lpvm_emu::{CompileOptions, EmuEngine};
            let opts = CompileOptions {
                float_mode,
                ..Default::default()
            };
            let engine = EmuEngine::new(opts);
            let module = engine.compile(ir, sig)
                .map_err(|e| anyhow::anyhow!("Emu compile failed: {e}"))?;
            module.debug_info()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Emu backend did not produce debug info"))
        }
        _ => anyhow::bail!("Unknown target: {}. Use rv32fa, rv32, or rv32lp", target),
    }
}
```

### 3. Create `lp-cli/src/commands/shader_debug/mod.rs`

```rust
//! shader-debug command - unified debug output for all backends.

pub mod args;
pub mod handler;

pub use handler::handle;
```

### 4. Update `lp-cli/src/commands/mod.rs`

Remove old modules, add new:
```rust
pub mod create;
pub mod dev;
pub mod heap_summary;
pub mod mem_profile;
pub mod serve;
pub mod shader_debug;  // NEW
pub mod shader_lpir;
pub mod upload;

// REMOVED: shader_rv32, shader_rv32fa
```

### 5. Update `lp-cli/src/main.rs`

Remove old commands, add new:
```rust
Commands::ShaderDebug(args) => {
    commands::shader_debug::handle(args)?;
}

// REMOVED: ShaderRv32, ShaderRv32Fa
```

### 6. Implement `ModuleDebugInfo::render()` and `help_text()`

In `lpvm/src/debug.rs`:

```rust
impl ModuleDebugInfo {
    pub fn render(&self, fn_filter: Option<&str>) -> String {
        let mut out = String::new();
        
        let functions_to_render: Vec<_> = if let Some(name) = fn_filter {
            self.functions.get(name).into_iter().collect()
        } else {
            self.functions.values().collect()
        };

        for (i, func) in functions_to_render.iter().enumerate() {
            if i > 0 {
                out.push_str("\n\n");
            }
            out.push_str(&format!("=== Function: {} ===\n\n", func.name));
            
            // Standard section order
            let section_order = &["interleaved", "disasm", "vinst", "liveness", "region"];
            
            for section_name in section_order {
                if let Some(content) = func.sections.get(*section_name) {
                    let count_line = if *section_name == "disasm" {
                        format!(" ({} instructions)", func.inst_count)
                    } else if *section_name == "interleaved" {
                        // Count VInsts in content
                        let vinst_count = content.lines().filter(|l| l.contains("= ")).count();
                        format!(" ({} VInsts)", vinst_count)
                    } else {
                        String::new()
                    };
                    
                    out.push_str(&format!("--- {}{} ---\n", section_name, count_line));
                    out.push_str(content);
                    out.push('\n');
                } else if *section_name == "interleaved" {
                    // Special message for missing interleaved
                    out.push_str(&format!("--- {} ---\n", section_name));
                    out.push_str("(not available for this backend - only disassembly available)\n\n");
                }
            }
        }
        
        out
    }

    pub fn help_text(&self, file_path: &str, target: &str) -> String {
        let mut out = String::new();
        
        out.push_str("────────────────────────────────────────\n");
        out.push_str("To show a specific function:\n");
        
        for func_name in self.function_names() {
            out.push_str(&format!(
                "  lp-cli shader-debug -t {} {} --fn {}\n",
                target, file_path, func_name
            ));
        }
        
        out.push('\n');
        out.push_str("Available functions: ");
        out.push_str(&self.function_names().join(", "));
        out.push('\n');
        
        out
    }
}
```

## Code Organization

- New module `shader_debug/` with clean structure (args.rs, handler.rs, mod.rs)
- No complex flag handling - just show everything
- Help text is auto-generated with copy-pasteable commands
- Backend selection is explicit with `-t` flag

## Tests

Add integration test:
```rust
#[test]
fn shader_debug_rv32fa() {
    // Run shader-debug on a test file
    let output = run_shader_debug("test.glsl", "rv32fa", None);
    assert!(output.contains("=== Function:"));
    assert!(output.contains("--- interleaved ---"));
    assert!(output.contains("--- disasm ---"));
    assert!(output.contains("To show a specific function:"));
}
```

## Validate

```bash
cargo build -p lp-cli
lp-cli shader-debug --help
lp-cli shader-debug -t rv32fa lp-shader/lps-filetests/filetests/debug/rainbow-noctrl-min.glsl
lp-cli shader-debug -t rv32 lp-shader/lps-filetests/filetests/debug/rainbow-noctrl-min.glsl
```

Ensure both FA and Cranelift paths work.
