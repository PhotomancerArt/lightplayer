# Design: Add VCode and Assembly Generation to Q32 Metrics

## Overview

Extend the `lp-glsl-q32-metrics-app` app to generate vcode and assembly files in addition to CLIF
files, enabling size comparison at multiple compilation stages. The app will switch from `JITModule`
to `ObjectModule` to access compiled code, use RISC-V 32-bit target, and add vcode/assembly size
metrics to statistics.

## File Structure

```
lp-glsl/lp-glsl-q32-metrics-app/
├── src/
│   ├── main.rs                    # UPDATE: Change to use ObjectModule
│   ├── cli.rs                     # No changes
│   ├── compiler.rs                # UPDATE: Use compile_to_gl_module_object, Target::riscv32_emulator()
│   ├── clif.rs                    # UPDATE: Rename to codegen.rs, add vcode/assembly writing
│   ├── stats.rs                   # UPDATE: Add vcode_size, assembly_size fields
│   └── report.rs                  # UPDATE: Include new size fields in reports
└── Cargo.toml                     # UPDATE: Add emulator feature to lp-glsl-compiler dependency
```

## Report Structure

```
reports/yyyy-mm-ddThh.mm.ss/
├── report.toml                    # UPDATE: Include vcode/assembly size metrics
└── test-add.glsl/                 # Per-test directory
    ├── test-add.glsl             # Copy of input GLSL
    ├── main.pre.clif              # Pre-transform CLIF
    ├── main.post.clif             # Post-transform CLIF
    ├── main.pre.vcode             # NEW: Pre-transform VCode
    ├── main.post.vcode            # NEW: Post-transform VCode
    ├── main.pre.s                 # NEW: Pre-transform assembly
    ├── main.post.s                # NEW: Post-transform assembly
    └── stats.toml                 # UPDATE: Include vcode/assembly sizes
```

## Code Structure

### Updated Types

**FunctionStats:**

```rust
pub struct FunctionStats {
    pub name: String,
    pub blocks: usize,
    pub instructions: usize,
    pub values: usize,
    pub clif_size: usize,
    pub vcode_size: usize,         // NEW: Size of vcode text
    pub assembly_size: usize,      // NEW: Size of assembly text
}
```

**ModuleStats:**

```rust
pub struct ModuleStats {
    pub total_blocks: usize,
    pub total_instructions: usize,
    pub total_values: usize,
    pub total_clif_size: usize,
    pub total_vcode_size: usize,   // NEW: Sum of all vcode sizes
    pub total_assembly_size: usize, // NEW: Sum of all assembly sizes
    pub functions: Vec<FunctionStats>,
}
```

**StatsDelta:**

```rust
pub struct StatsDelta {
    pub blocks: i32,
    pub instructions: i32,
    pub values: i32,
    pub clif_size: i32,
    pub vcode_size: i32,            // NEW: Delta for vcode size
    pub assembly_size: i32,         // NEW: Delta for assembly size
    pub blocks_percent: f64,
    pub instructions_percent: f64,
    pub values_percent: f64,
    pub clif_size_percent: f64,
    pub vcode_size_percent: f64,    // NEW: Percentage change for vcode
    pub assembly_size_percent: f64, // NEW: Percentage change for assembly
}
```

### Updated Functions

**compiler.rs:**

```rust
// UPDATE: Change return type from JITModule to ObjectModule
pub fn compile_and_transform(
    glsl_source: &str,
    format: FixedPointFormat,
) -> Result<(GlModule<ObjectModule>, GlModule<ObjectModule>)> {
    // UPDATE: Use riscv32_emulator target instead of host_jit
    let target = Target::riscv32_emulator()
        .map_err(|e| anyhow::anyhow!("Failed to create target: {}", e))?;

    // UPDATE: Use compile_to_gl_module_object instead of compile_to_gl_module_jit
    let mut compiler_before = GlslCompiler::new();
    let module_before = compiler_before
        .compile_to_gl_module_object(glsl_source, target.clone())
        .map_err(|e| anyhow::anyhow!("Failed to compile GLSL: {}", e))?;

    let mut compiler_after = GlslCompiler::new();
    let module_for_transform = compiler_after
        .compile_to_gl_module_object(glsl_source, target)
        .map_err(|e| anyhow::anyhow!("Failed to compile GLSL (for transform): {}", e))?;

    let transform = Q32Transform::new(format);
    let module_after = module_for_transform
        .apply_transform(transform)
        .map_err(|e| anyhow::anyhow!("Failed to apply q32 transform: {}", e))?;

    Ok((module_before, module_after))
}
```

**clif.rs (rename to codegen.rs):**

```rust
// NEW: Extract vcode and assembly from compiled function
fn extract_compiled_code(
    ctx: &mut cranelift_codegen::Context,
    module: &GlModule<ObjectModule>,
    name: &str,
) -> Result<(Option<String>, Option<String>)> {
    let (vcode, disasm) = if let Some(compiled_code) = ctx.compiled_code() {
        // Get VCode
        let vcode = compiled_code.vcode.as_ref().map(|s| s.clone());

        // Try to generate RISC-V disassembly using Capstone
        let disasm = {
            let module_ref = module.module_internal();
            let isa = module_ref.isa();
            if let Ok(cs) = isa.to_capstone() {
                if let Ok(disasm_str) = compiled_code.disassemble(Some(&ctx.func.params), &cs) {
                    Some(disasm_str)
                } else {
                    vcode.clone() // Fallback to vcode if disassembly fails
                }
            } else {
                vcode.clone() // Fallback to vcode if Capstone unavailable
            }
        };

        (vcode, disasm)
    } else {
        (None, None)
    };

    Ok((vcode, disasm))
}

// NEW: Compile function and extract vcode/assembly
fn compile_function_and_extract(
    module: &mut GlModule<ObjectModule>,
    name: &str,
    func: cranelift_codegen::ir::Function,
    func_id: cranelift_module::FuncId,
) -> Result<(Option<String>, Option<String>)> {
    // Create context
    let mut ctx = {
        let module_ref = module.module_internal();
        module_ref.make_context()
    };
    ctx.func = func;

    // Enable disassembly
    ctx.set_disasm(true);

    // Define function (compiles it)
    module
        .module_mut_internal()
        .define_function(func_id, &mut ctx)
        .map_err(|e| anyhow::anyhow!("Failed to define function {}: {}", name, e))?;

    // Extract vcode and assembly
    let result = extract_compiled_code(&mut ctx, module, name)?;

    // Clear context
    {
        let module_ref = module.module_internal();
        module_ref.clear_context(&mut ctx);
    }

    Ok(result)
}

// UPDATE: Write CLIF, vcode, and assembly files
pub fn write_codegen_files(
    test_dir: &Path,
    module_before: &mut GlModule<ObjectModule>,
    module_after: &mut GlModule<ObjectModule>,
    verbose: bool,
) -> Result<HashMap<String, (usize, usize)>> {
    // Build name mappings for CLIF formatting
    let mut name_mapping_before: HashMap<String, String> = HashMap::new();
    for (name, gl_func) in &module_before.fns {
        name_mapping_before.insert(gl_func.func_id.as_u32().to_string(), name.clone());
    }

    let mut name_mapping_after: HashMap<String, String> = HashMap::new();
    for (name, gl_func) in &module_after.fns {
        name_mapping_after.insert(gl_func.func_id.as_u32().to_string(), name.clone());
    }

    // Sort function names for deterministic output
    let mut func_names: Vec<String> = module_before.fns.keys().cloned().collect();
    func_names.sort();

    let mut vcode_assembly_sizes: HashMap<String, (usize, usize)> = HashMap::new();

    for name in &func_names {
        if let Some(gl_func_before) = module_before.fns.get(name)
            && let Some(gl_func_after) = module_after.fns.get(name)
        {
            // Write CLIF files (existing logic)
            let clif_before = format_function(&gl_func_before.function, name, &name_mapping_before)
                .map_err(|e| anyhow::anyhow!("Failed to format function {} (before): {}", name, e))?;
            let clif_after = format_function(&gl_func_after.function, name, &name_mapping_after)
                .map_err(|e| anyhow::anyhow!("Failed to format function {} (after): {}", name, e))?;

            fs::write(test_dir.join(format!("{}.pre.clif", name)), &clif_before)?;
            fs::write(test_dir.join(format!("{}.post.clif", name)), &clif_after)?;

            // NEW: Compile and extract vcode/assembly for before
            let (vcode_before, asm_before) = compile_function_and_extract(
                module_before,
                name,
                gl_func_before.function.clone(),
                gl_func_before.func_id,
            )?;

            // NEW: Compile and extract vcode/assembly for after
            let (vcode_after, asm_after) = compile_function_and_extract(
                module_after,
                name,
                gl_func_after.function.clone(),
                gl_func_after.func_id,
            )?;

            // Write vcode files
            if let Some(ref vcode) = vcode_before {
                fs::write(test_dir.join(format!("{}.pre.vcode", name)), vcode)?;
            }
            if let Some(ref vcode) = vcode_after {
                fs::write(test_dir.join(format!("{}.post.vcode", name)), vcode)?;
            }

            // Write assembly files
            if let Some(ref asm) = asm_before {
                fs::write(test_dir.join(format!("{}.pre.s", name)), asm)?;
            }
            if let Some(ref asm) = asm_after {
                fs::write(test_dir.join(format!("{}.post.s", name)), asm)?;
            }

            // Calculate sizes for statistics
            let vcode_size = vcode_after.as_ref().map(|s| s.len()).unwrap_or(0);
            let assembly_size = asm_after.as_ref().map(|s| s.len()).unwrap_or(0);
            vcode_assembly_sizes.insert(name.clone(), (vcode_size, assembly_size));

            if verbose {
                eprintln!("  Wrote {}.pre.clif, {}.post.clif, vcode, and assembly files", name, name);
            }
        }
    }

    Ok(vcode_assembly_sizes)
}
```

**stats.rs:**

```rust
// UPDATE: Add vcode_size and assembly_size parameters
pub fn collect_function_stats(
    func: &Function,
    name: &str,
    name_mapping: &HashMap<String, String>,
    vcode_size: usize,      // NEW: Size of vcode text
    assembly_size: usize,    // NEW: Size of assembly text
) -> Result<FunctionStats> {
    // ... existing code ...
    
    Ok(FunctionStats {
        name: name.to_string(),
        blocks: num_blocks,
        instructions: num_insts,
        values: num_values,
        clif_size,
        vcode_size,          // NEW
        assembly_size,       // NEW
    })
}

// UPDATE: Pass vcode/assembly sizes when collecting stats
pub fn collect_module_stats(
    module: &GlModule<ObjectModule>,
    vcode_assembly_sizes: &HashMap<String, (usize, usize)>,
) -> Result<ModuleStats> {
    // ... existing code ...
    
    for name in &func_names {
        if let Some(gl_func) = module.fns.get(name) {
            let (vcode_size, assembly_size) = vcode_assembly_sizes
                .get(name)
                .copied()
                .unwrap_or((0, 0));
            
            let stats = collect_function_stats(
                &gl_func.function,
                name,
                &name_mapping,
                vcode_size,
                assembly_size,
            )?;
            
            total_vcode_size += stats.vcode_size;
            total_assembly_size += stats.assembly_size;
            // ... rest of existing code ...
        }
    }
    
    Ok(ModuleStats {
        // ... existing fields ...
        total_vcode_size,
        total_assembly_size,
        functions,
    })
}

// UPDATE: Add vcode/assembly delta calculations
pub fn calculate_deltas(before: &ModuleStats, after: &ModuleStats) -> StatsDelta {
    // ... existing code ...
    
    let vcode_size_diff = after.total_vcode_size as i32 - before.total_vcode_size as i32;
    let assembly_size_diff = after.total_assembly_size as i32 - before.total_assembly_size as i32;
    
    let vcode_size_percent = if before.total_vcode_size > 0 {
        (vcode_size_diff as f64 / before.total_vcode_size as f64) * 100.0
    } else {
        0.0
    };
    
    let assembly_size_percent = if before.total_assembly_size > 0 {
        (assembly_size_diff as f64 / before.total_assembly_size as f64) * 100.0
    } else {
        0.0
    };
    
    StatsDelta {
        // ... existing fields ...
        vcode_size: vcode_size_diff,
        assembly_size: assembly_size_diff,
        vcode_size_percent,
        assembly_size_percent,
    }
}
```

**main.rs:**

```rust
// UPDATE: Change type from JITModule to ObjectModule
fn process_test(
    test_name: &str,
    test_dir: &Path,
    module_before: &mut GlModule<ObjectModule>,  // UPDATE: Changed from &GlModule<JITModule>
    module_after: &mut GlModule<ObjectModule>,   // UPDATE: Changed from &GlModule<JITModule>
    _glsl_source: &str,
    verbose: bool,
) -> Result<report::TestSummary> {
    // NEW: Write codegen files (CLIF, vcode, assembly) and get sizes
    let vcode_assembly_sizes = codegen::write_codegen_files(
        test_dir,
        module_before,
        module_after,
        verbose,
    )?;

    // UPDATE: Pass vcode/assembly sizes when collecting stats
    let stats_before = stats::collect_module_stats(module_before, &vcode_assembly_sizes)?;
    let stats_after = stats::collect_module_stats(module_after, &vcode_assembly_sizes)?;
    let delta = stats::calculate_deltas(&stats_before, &stats_after);

    // ... rest of existing code ...
}
```

**Cargo.toml:**

```toml
[dependencies]
lp-glsl-compiler = { path = "../../crates/lp-glsl-compiler", default-features = false, features = ["std", "emulator"] }  # UPDATE: Add emulator feature
```

## Implementation Notes

1. **Module Type Change**: Switch from `JITModule` to `ObjectModule` to access compiled code via
   `ctx.compiled_code()`.

2. **Target Change**: Use `Target::riscv32_emulator()` instead of `Target::host_jit()` to generate
   RISC-V 32-bit code.

3. **Compilation**: Functions must be compiled (via `define_function`) to generate vcode and
   assembly. This happens after CLIF IR is generated but before statistics are collected.

4. **Disassembly**: Use Capstone disassembler (via `isa.to_capstone()`) to generate real RISC-V
   assembly. Fall back to vcode if Capstone fails.

5. **Size Metrics**: Track vcode and assembly text sizes (in bytes) similar to CLIF size, and
   include deltas and percentages in statistics.

6. **File Naming**: Use `.vcode` extension for vcode files and `.s` extension for assembly files,
   following the pattern of `.pre` and `.post` suffixes.

7. **Feature Requirement**: The `emulator` feature must be enabled in `lp-glsl-compiler` dependency
   to access RISC-V target and Capstone disassembly.

## Success Criteria

- App compiles with `emulator` feature enabled
- VCode files are generated for all functions (before and after transform)
- Assembly files are generated for all functions (before and after transform)
- Statistics include vcode_size and assembly_size fields
- Report TOML files include vcode and assembly size metrics
- All existing functionality (CLIF generation, statistics) continues to work
