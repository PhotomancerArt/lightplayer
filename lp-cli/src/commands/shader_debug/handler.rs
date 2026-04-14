//! Handler for `shader-debug`.

use anyhow::{Context, Result};
use lpir::{FloatMode, validate_module};

use super::args::{Args, BackendTarget};

pub fn handle_shader_debug(args: Args) -> Result<()> {
    let src = std::fs::read_to_string(&args.input)
        .with_context(|| format!("read {}", args.input.display()))?;

    let naga = lps_frontend::compile(&src).context("GLSL parse (Naga)")?;
    let (ir, sig) = lps_frontend::lower(&naga).context("lower to LPIR")?;

    if let Err(errs) = validate_module(&ir) {
        anyhow::bail!(
            "LPIR validation failed:\n{}",
            errs.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    let float_mode = match args.float_mode.as_str() {
        "q32" => FloatMode::Q32,
        "f32" => FloatMode::F32,
        _ => anyhow::bail!("invalid --float-mode (use q32 or f32)"),
    };

    // Build sig map for looking up function signatures
    let sig_map: std::collections::BTreeMap<&str, &lps_frontend::LpsFnSig> =
        sig.functions.iter().map(|s| (s.name.as_str(), s)).collect();

    let file_path_str = args.input.to_string_lossy().to_string();

    match args.target {
        BackendTarget::Rv32fa => {
            print_fa_debug(&ir, &sig, &sig_map, float_mode, args.func.as_deref(), &file_path_str)?;
        }
        BackendTarget::Rv32lp => {
            print_linear_debug(&ir, &sig, &sig_map, float_mode, args.func.as_deref(), &file_path_str)?;
        }
        BackendTarget::Rv32 => {
            print_cranelift_debug(&ir, &sig, float_mode, args.func.as_deref(), &file_path_str, false)?;
        }
        BackendTarget::Emu => {
            print_cranelift_debug(&ir, &sig, float_mode, args.func.as_deref(), &file_path_str, true)?;
        }
    }

    Ok(())
}

/// Print debug info for fastalloc backend.
fn print_fa_debug(
    ir: &lpir::LpirModule,
    sig: &lps_frontend::LpsModuleSig,
    sig_map: &std::collections::BTreeMap<&str, &lps_frontend::LpsFnSig>,
    float_mode: FloatMode,
    func_filter: Option<&str>,
    file_path_str: &str,
) -> Result<()> {
    use lpvm_native_fa::abi::ModuleAbi;
    use lpvm_native_fa::fa_alloc::render::render_interleaved;
    use lpvm_native_fa::fa_alloc::allocate;
    use lpvm_native_fa::lower_ops;
    use lpvm_native_fa::rv32::abi::func_abi_rv32;
    use lpvm_native_fa::rv32::emit::emit_function;

    let module_abi = ModuleAbi::from_ir_and_sig(ir, sig);

    // Collect stats for summary table first
    let mut stats: Vec<(String, usize, usize)> = Vec::new();
    for func in &ir.functions {
        if let Some(name) = func_filter {
            if func.name != name {
                continue;
            }
        }

        let lpir_count = func.body.len();

        let default_sig = lps_frontend::LpsFnSig {
            name: func.name.clone(),
            return_type: lps_frontend::LpsType::Void,
            parameters: Vec::new(),
        };
        let fn_sig = sig_map.get(func.name.as_str()).copied().unwrap_or(&default_sig);

        // Compile to get instruction count
        let slots = func.total_param_slots() as usize;
        let func_abi = func_abi_rv32(fn_sig, slots);

        let lowered = lower_ops(func, ir, &module_abi, float_mode)
            .map_err(|e| anyhow::anyhow!("lower: {e:?}"))?;
        let alloc_result = allocate(&lowered, &func_abi)
            .map_err(|e| anyhow::anyhow!("fastalloc: {e}"))?;

        let mut used_callee_saved = alloc_result.used_callee_saved;
        if func_abi.is_sret() {
            use lpvm_native_fa::abi::PregSet;
            use lpvm_native_fa::rv32::abi::S1;
            used_callee_saved = used_callee_saved.union(PregSet::singleton(S1));
        }
        let caller_outgoing_stack_bytes = max_outgoing_stack_bytes(&lowered.vinsts);
        let is_leaf = !contains_call(&lowered.vinsts);
        let frame = lpvm_native_fa::abi::FrameLayout::compute(
            &func_abi,
            alloc_result.spill_slots,
            used_callee_saved,
            &lowered.lpir_slots,
            is_leaf,
            module_abi.max_callee_sret_bytes(),
            caller_outgoing_stack_bytes,
        );

        let emitted = emit_function(
            &lowered.vinsts,
            &lowered.vreg_pool,
            &alloc_result.output,
            frame,
            &lowered.symbols,
            func_abi.is_sret(),
        )
        .map_err(|e| anyhow::anyhow!("emit: {e:?}"))?;

        let disasm_count = emitted.code.len() / 4;
        stats.push((func.name.clone(), lpir_count, disasm_count));
    }

    // Print summary table
    print_summary_table(&stats, "rv32fa");

    let mut first = true;
    for func in &ir.functions {
        // Filter if specified
        if let Some(name) = func_filter {
            if func.name != name {
                continue;
            }
        }

        if !first {
            println!();
            println!();
        }
        first = false;

        println!("=== Function: {} ===", func.name);
        println!();

        let default_sig = lps_frontend::LpsFnSig {
            name: func.name.clone(),
            return_type: lps_frontend::LpsType::Void,
            parameters: Vec::new(),
        };
        let fn_sig = sig_map.get(func.name.as_str()).copied().unwrap_or(&default_sig);

        // Lower
        let lowered = lower_ops(func, ir, &module_abi, float_mode)
            .map_err(|e| anyhow::anyhow!("lower: {e:?}"))?;

        // Allocate
        let slots = func.total_param_slots() as usize;
        let func_abi = func_abi_rv32(fn_sig, slots);
        let alloc_result = allocate(&lowered, &func_abi)
            .map_err(|e| anyhow::anyhow!("fastalloc: {e}"))?;

        // Interleaved section
        let interleaved = render_interleaved(
            func,
            ir,
            &lowered.vinsts,
            &lowered.vreg_pool,
            &alloc_result.output,
            &func_abi,
            &lowered.symbols,
        );

        let vinst_count = interleaved.lines().filter(|l| l.contains(" = ")).count();
        println!("--- interleaved ({} VInsts) ---", vinst_count);
        println!("{}", interleaved);

        // Emit to get machine code
        let mut used_callee_saved = alloc_result.used_callee_saved;
        if func_abi.is_sret() {
            use lpvm_native_fa::abi::PregSet;
            use lpvm_native_fa::rv32::abi::S1;
            used_callee_saved = used_callee_saved.union(PregSet::singleton(S1));
        }
        let caller_outgoing_stack_bytes = max_outgoing_stack_bytes(&lowered.vinsts);
        let is_leaf = !contains_call(&lowered.vinsts);
        let frame = lpvm_native_fa::abi::FrameLayout::compute(
            &func_abi,
            alloc_result.spill_slots,
            used_callee_saved,
            &lowered.lpir_slots,
            is_leaf,
            module_abi.max_callee_sret_bytes(),
            caller_outgoing_stack_bytes,
        );

        let emitted = emit_function(
            &lowered.vinsts,
            &lowered.vreg_pool,
            &alloc_result.output,
            frame,
            &lowered.symbols,
            func_abi.is_sret(),
        )
        .map_err(|e| anyhow::anyhow!("emit: {e:?}"))?;

        // Disasm section
        let code = &emitted.code;
        let inst_count = code.len() / 4;
        println!("--- disasm ({} instructions) ---", inst_count);
        let mut off = 0usize;
        while off + 4 <= code.len() {
            let w = u32::from_le_bytes(code[off..off + 4].try_into().expect("4 bytes"));
            println!("{:04x}\t{:08x}\t{}", off, w, lp_riscv_inst::format_instruction(w));
            off += 4;
        }
        println!();
    }

    // Print help text if showing all functions and there's more than one
    if func_filter.is_none() && ir.functions.len() > 1 {
        println!("────────────────────────────────────────");
        println!("To show a specific function:");
        for func in &ir.functions {
            println!("  lp-cli shader-debug -t rv32fa {} --fn {}", file_path_str, func.name);
        }
        println!();
        print!("Available functions: ");
        let names: Vec<_> = ir.functions.iter().map(|f| f.name.as_str()).collect();
        println!("{}", names.join(", "));
    }

    Ok(())
}

/// Print debug info for linear scan backend.
fn print_linear_debug(
    ir: &lpir::LpirModule,
    sig: &lps_frontend::LpsModuleSig,
    sig_map: &std::collections::BTreeMap<&str, &lps_frontend::LpsFnSig>,
    float_mode: FloatMode,
    func_filter: Option<&str>,
    file_path_str: &str,
) -> Result<()> {
    use lpvm_native::abi::ModuleAbi;
    use lpvm_native::isa::rv32::debug::disasm::{DisasmOptions, disassemble_function};
    use lpvm_native::isa::rv32::debug::LineTable;
    use lpvm_native::isa::rv32::emit::emit_function_bytes;

    let module_abi = ModuleAbi::from_ir_and_sig(ir, sig);

    // Collect stats for summary table
    let mut stats: Vec<(String, usize, usize)> = Vec::new();
    for func in &ir.functions {
        if let Some(name) = func_filter {
            if func.name != name {
                continue;
            }
        }

        let lpir_count = func.body.len();

        let default_sig = lps_frontend::LpsFnSig {
            name: func.name.clone(),
            return_type: lps_frontend::LpsType::Void,
            parameters: Vec::new(),
        };
        let fn_sig = sig_map.get(func.name.as_str()).copied().unwrap_or(&default_sig);

        // Compile to get instruction count
        let emitted =
            emit_function_bytes(func, ir, &module_abi, fn_sig, float_mode, true, false)
                .map_err(|e| anyhow::anyhow!("emit: {e:?}"))?;

        let disasm_count = emitted.code.len() / 4;
        stats.push((func.name.clone(), lpir_count, disasm_count));
    }

    print_summary_table(&stats, "rv32lp");

    let mut first = true;
    for func in &ir.functions {
        // Filter if specified
        if let Some(name) = func_filter {
            if func.name != name {
                continue;
            }
        }

        if !first {
            println!();
            println!();
        }
        first = false;

        println!("=== Function: {} ===", func.name);
        println!();

        let default_sig = lps_frontend::LpsFnSig {
            name: func.name.clone(),
            return_type: lps_frontend::LpsType::Void,
            parameters: Vec::new(),
        };
        let fn_sig = sig_map.get(func.name.as_str()).copied().unwrap_or(&default_sig);

        // Emit and get debug info
        let emitted = emit_function_bytes(func, ir, &module_abi, fn_sig, float_mode, true, true)
            .map_err(|e| anyhow::anyhow!("emit: {e:?}"))?;

        // Note: interleaved not available for linear scan backend
        println!("--- interleaved ---");
        println!("(not available for this backend - only disassembly available)");
        println!();

        // Disasm section
        let table = LineTable::from_debug_lines(&emitted.debug_lines);
        let disasm = disassemble_function(&emitted.code, &table, func, DisasmOptions::default());
        let inst_count = emitted.code.len() / 4;
        println!("--- disasm ({} instructions) ---", inst_count);
        println!("{}", disasm);
    }

    // Print help text if showing all functions and there's more than one
    if func_filter.is_none() && ir.functions.len() > 1 {
        println!("────────────────────────────────────────");
        println!("To show a specific function:");
        for func in &ir.functions {
            println!("  lp-cli shader-debug -t rv32lp {} --fn {}", file_path_str, func.name);
        }
        println!();
        print!("Available functions: ");
        let names: Vec<_> = ir.functions.iter().map(|f| f.name.as_str()).collect();
        println!("{}", names.join(", "));
    }

    Ok(())
}

/// Max bytes needed at `[SP+0]` for outgoing stack-passed call arguments.
fn max_outgoing_stack_bytes(vinsts: &[lpvm_native_fa::vinst::VInst]) -> u32 {
    use lpvm_native_fa::rv32::abi::ARG_REGS;
    let mut max_bytes = 0u32;
    for inst in vinsts {
        if let lpvm_native_fa::vinst::VInst::Call {
            args,
            callee_uses_sret,
            ..
        } = inst
        {
            let cap = if *callee_uses_sret {
                ARG_REGS.len() - 1
            } else {
                ARG_REGS.len()
            };
            let n = args.len();
            if n > cap {
                let stack_words = (n - cap) as u32;
                max_bytes = max_bytes.max(stack_words * 4);
            }
        }
    }
    max_bytes
}

/// Returns true if the function contains any call instructions.
fn contains_call(vinsts: &[lpvm_native_fa::vinst::VInst]) -> bool {
    use lpvm_native_fa::vinst::VInst;
    vinsts.iter().any(|inst| matches!(inst, VInst::Call { .. }))
}

/// Print debug info for Cranelift-based backends (rv32 and emu).
fn print_cranelift_debug(
    ir: &lpir::LpirModule,
    sig: &lps_frontend::LpsModuleSig,
    float_mode: FloatMode,
    func_filter: Option<&str>,
    file_path_str: &str,
    _is_emu: bool,
) -> Result<()> {
    use lpvm_cranelift::{object_bytes_from_ir, link_object_with_builtins, CompileOptions};

    let options = CompileOptions {
        float_mode,
        ..CompileOptions::default()
    };

    // Compile to object bytes
    let object_bytes = object_bytes_from_ir(ir, &options)
        .map_err(|e| anyhow::anyhow!("cranelift compile: {e}"))?;

    // Link with builtins to get loadable code
    let elf_info = link_object_with_builtins(&object_bytes)
        .map_err(|e| anyhow::anyhow!("cranelift link: {e}"))?;

    // Collect stats for summary table
    let mut stats: Vec<(String, usize, usize)> = Vec::new();
    for func in &ir.functions {
        if let Some(name) = func_filter {
            if func.name != name {
                continue;
            }
        }

        let lpir_count = func.body.len();

        // Look up function address in symbol map
        let addr = elf_info.symbol_map.get(&func.name).copied();
        let disasm_count = match addr {
            Some(addr) => {
                // Find function size by looking at next symbol
                let mut end_addr = elf_info.code.len() as u32;
                for (sym_name, sym_addr) in &elf_info.symbol_map {
                    if *sym_addr > addr && *sym_addr < end_addr && sym_name != &func.name {
                        end_addr = *sym_addr;
                    }
                }

                let start = addr as usize;
                let end = end_addr as usize;
                let func_code = if start < elf_info.code.len() {
                    let actual_end = end.min(elf_info.code.len());
                    &elf_info.code[start..actual_end]
                } else {
                    &[] as &[u8]
                };

                func_code.len() / 4
            }
            None => 0,
        };

        stats.push((func.name.clone(), lpir_count, disasm_count));
    }

    let target_name = if _is_emu { "emu" } else { "rv32" };
    print_summary_table(&stats, target_name);

    let mut first = true;
    for func in &ir.functions {
        // Filter if specified
        if let Some(name) = func_filter {
            if func.name != name {
                continue;
            }
        }

        if !first {
            println!();
            println!();
        }
        first = false;

        println!("=== Function: {} ===", func.name);
        println!();

        // Interleaved not available for Cranelift
        println!("--- interleaved ---");
        println!("(not available for Cranelift - only disassembly available)");
        println!();

        // Look up function address in symbol map
        let addr = elf_info.symbol_map.get(&func.name).copied();

        let disasm = match addr {
            Some(addr) => {
                // Find function size by looking at next symbol
                let mut end_addr = elf_info.code.len() as u32;
                for (sym_name, sym_addr) in &elf_info.symbol_map {
                    if *sym_addr > addr && *sym_addr < end_addr && sym_name != &func.name {
                        end_addr = *sym_addr;
                    }
                }

                let start = addr as usize;
                let end = end_addr as usize;
                let func_code = if start < elf_info.code.len() {
                    let actual_end = end.min(elf_info.code.len());
                    &elf_info.code[start..actual_end]
                } else {
                    &[] as &[u8]
                };

                disassemble_raw(func_code)
            }
            None => format!("; Function {} not found in symbol map", func.name),
        };

        let inst_count = disasm.lines().count();
        println!("--- disasm ({} instructions) ---", inst_count);
        println!("{}", disasm);
    }

    // Print help text if showing all functions and there's more than one
    if func_filter.is_none() && ir.functions.len() > 1 {
        let target = if _is_emu { "emu" } else { "rv32" };
        println!("────────────────────────────────────────");
        println!("To show a specific function:");
        for func in &ir.functions {
            println!("  lp-cli shader-debug -t {} {} --fn {}", target, file_path_str, func.name);
        }
        println!();
        print!("Available functions: ");
        let names: Vec<_> = ir.functions.iter().map(|f| f.name.as_str()).collect();
        println!("{}", names.join(", "));
    }

    Ok(())
}

/// Disassemble raw bytes without source annotations.
fn disassemble_raw(code: &[u8]) -> String {
    use lp_riscv_inst::format_instruction;

    let mut out = String::new();
    let mut offset = 0usize;

    while offset + 4 <= code.len() {
        let word = u32::from_le_bytes([
            code[offset],
            code[offset + 1],
            code[offset + 2],
            code[offset + 3],
        ]);
        let asm = format_instruction(word);
        out.push_str(&format!("{:04x}\t{:08x}\t{}\n", offset, word, asm));
        offset += 4;
    }

    out
}

/// Print summary table of function instruction counts.
fn print_summary_table(stats: &[(String, usize, usize)], backend: &str) {
    if stats.is_empty() {
        return;
    }

    println!("=== Summary: {} functions ===", backend);
    println!();

    // Find max name length for alignment
    let max_name_len = stats.iter().map(|(name, _, _)| name.len()).max().unwrap_or(20);
    let name_width = max_name_len.max(20);

    // Header
    println!("{:<name_width$}  {:>12}  {:>12}", "Function", "LPIR", "Disasm");
    println!("{}", "-".repeat(name_width + 12 + 12 + 4));

    // Rows
    let mut total_lpir = 0;
    let mut total_disasm = 0;
    for (name, lpir, disasm) in stats {
        println!("{:<name_width$}  {:>12}  {:>12}", name, lpir, disasm);
        total_lpir += lpir;
        total_disasm += disasm;
    }

    // Total row
    println!("{}", "-".repeat(name_width + 12 + 12 + 4));
    println!("{:<name_width$}  {:>12}  {:>12}", "TOTAL", total_lpir, total_disasm);
    println!();
    println!();
}
