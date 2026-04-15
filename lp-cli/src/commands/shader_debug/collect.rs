//! Backend-specific data collectors for shader-debug.

use anyhow::Result;
use lpir::{FloatMode, LpirModule};
use lps_frontend::LpsModuleSig;

use super::types::{BackendDebugData, FunctionDebugData};

/// Collect debug data from the fastalloc backend.
pub fn collect_fa_data(
    ir: &LpirModule,
    sig: &LpsModuleSig,
    float_mode: FloatMode,
    func_filter: Option<&str>,
) -> Result<BackendDebugData> {
    use lpvm_native_fa::abi::ModuleAbi;
    use lpvm_native_fa::fa_alloc::allocate;
    use lpvm_native_fa::fa_alloc::render::render_interleaved;
    use lpvm_native_fa::lower_ops;
    use lpvm_native_fa::rv32::abi::func_abi_rv32;
    use lpvm_native_fa::rv32::emit::emit_function;

    let module_abi = ModuleAbi::from_ir_and_sig(ir, sig);

    let sig_map: std::collections::BTreeMap<&str, &lps_frontend::LpsFnSig> =
        sig.functions.iter().map(|s| (s.name.as_str(), s)).collect();

    let mut backend_data = BackendDebugData::new("rv32fa");

    for func in &ir.functions {
        // Filter if specified
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
        let fn_sig = sig_map
            .get(func.name.as_str())
            .copied()
            .unwrap_or(&default_sig);

        // Lower and compile
        let lowered = lower_ops(func, ir, &module_abi, float_mode)
            .map_err(|e| anyhow::anyhow!("lower: {e:?}"))?;

        let slots = func.total_param_slots() as usize;
        let func_abi = func_abi_rv32(fn_sig, slots);
        let alloc_result =
            allocate(&lowered, &func_abi).map_err(|e| anyhow::anyhow!("fastalloc: {e}"))?;

        // Generate interleaved output
        let interleaved = render_interleaved(
            func,
            ir,
            &lowered.vinsts,
            &lowered.vreg_pool,
            &alloc_result.output,
            &func_abi,
            &lowered.symbols,
        );

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

        let disasm_count = emitted.code.len() / 4;

        // Generate disassembly
        let mut disasm = String::new();
        let mut off = 0usize;
        while off + 4 <= emitted.code.len() {
            let w = u32::from_le_bytes(emitted.code[off..off + 4].try_into().expect("4 bytes"));
            disasm.push_str(&format!(
                "{:04x}\t{:08x}\t{}\n",
                off,
                w,
                lp_riscv_inst::format_instruction(w)
            ));
            off += 4;
        }

        let mut func_data = FunctionDebugData::new(func.name.clone());
        func_data.lpir_count = lpir_count;
        func_data.disasm_count = disasm_count;
        func_data.spill_slots = Some(alloc_result.spill_slots as usize);
        func_data.interleaved = Some(interleaved);
        func_data.disasm = disasm;
        func_data.has_vinst = true;

        backend_data.functions.push(func_data);
    }

    Ok(backend_data)
}

/// Collect debug data from the linear scan backend.
pub fn collect_linear_data(
    ir: &LpirModule,
    sig: &LpsModuleSig,
    float_mode: FloatMode,
    func_filter: Option<&str>,
) -> Result<BackendDebugData> {
    use lpvm_native::abi::ModuleAbi;
    use lpvm_native::isa::rv32::debug::LineTable;
    use lpvm_native::isa::rv32::debug::disasm::{DisasmOptions, disassemble_function};
    use lpvm_native::isa::rv32::emit::emit_function_bytes;

    let module_abi = ModuleAbi::from_ir_and_sig(ir, sig);

    let sig_map: std::collections::BTreeMap<&str, &lps_frontend::LpsFnSig> =
        sig.functions.iter().map(|s| (s.name.as_str(), s)).collect();

    let mut backend_data = BackendDebugData::new("rv32lp");

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
        let fn_sig = sig_map
            .get(func.name.as_str())
            .copied()
            .unwrap_or(&default_sig);

        let emitted = emit_function_bytes(func, ir, &module_abi, fn_sig, float_mode, true, false)
            .map_err(|e| anyhow::anyhow!("emit: {e:?}"))?;

        let disasm_count = emitted.code.len() / 4;

        let table = LineTable::from_debug_lines(&emitted.debug_lines);
        let disasm = disassemble_function(&emitted.code, &table, func, DisasmOptions::default());

        let mut func_data = FunctionDebugData::new(func.name.clone());
        func_data.lpir_count = lpir_count;
        func_data.disasm_count = disasm_count;
        func_data.disasm = disasm;
        func_data.has_vinst = false;

        backend_data.functions.push(func_data);
    }

    Ok(backend_data)
}

/// Collect debug data from Cranelift-based backends (rv32 and emu).
pub fn collect_cranelift_data(
    ir: &LpirModule,
    _sig: &LpsModuleSig,
    float_mode: FloatMode,
    func_filter: Option<&str>,
    is_emu: bool,
) -> Result<BackendDebugData> {
    use lpvm_cranelift::{CompileOptions, link_object_with_builtins, object_bytes_from_ir};

    let options = CompileOptions {
        float_mode,
        ..CompileOptions::default()
    };

    let object_bytes = object_bytes_from_ir(ir, &options)
        .map_err(|e| anyhow::anyhow!("cranelift compile: {e}"))?;

    let elf_info = link_object_with_builtins(&object_bytes)
        .map_err(|e| anyhow::anyhow!("cranelift link: {e}"))?;

    let backend_name = if is_emu { "emu" } else { "rv32" };
    let mut backend_data = BackendDebugData::new(backend_name);

    for func in &ir.functions {
        if let Some(name) = func_filter {
            if func.name != name {
                continue;
            }
        }

        let lpir_count = func.body.len();

        let addr = elf_info.symbol_map.get(&func.name).copied();
        let (disasm, disasm_count) = match addr {
            Some(addr) => {
                let mut end_addr = elf_info.code.len() as u32;
                // Only consider symbols from the user object file (not builtins)
                // when determining function end address.
                // Also skip Cranelift internal jump labels (starting with ".L")
                for (sym_name, sym_addr) in &elf_info.symbol_map {
                    if *sym_addr > addr && *sym_addr < end_addr && sym_name != &func.name {
                        // Only use this symbol if it's from the user code section
                        // and not an internal Cranelift label
                        if *sym_addr >= elf_info.user_code_start
                            && !sym_name.starts_with(".L")
                        {
                            end_addr = *sym_addr;
                        }
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

                let d = disassemble_raw(func_code);
                let count = d.lines().count();
                (d, count)
            }
            None => (
                format!("; Function {} not found in symbol map", func.name),
                0,
            ),
        };

        let mut func_data = FunctionDebugData::new(func.name.clone());
        func_data.lpir_count = lpir_count;
        func_data.disasm_count = disasm_count;
        func_data.disasm = disasm;
        func_data.has_vinst = false;

        backend_data.functions.push(func_data);
    }

    Ok(backend_data)
}

/// Disassemble raw bytes without source annotations.
fn disassemble_raw(code: &[u8]) -> String {
    let mut out = String::new();
    let mut offset = 0usize;

    while offset + 4 <= code.len() {
        let word = u32::from_le_bytes([
            code[offset],
            code[offset + 1],
            code[offset + 2],
            code[offset + 3],
        ]);
        let asm = lp_riscv_inst::format_instruction(word);
        out.push_str(&format!("{:04x}\t{:08x}\t{}\n", offset, word, asm));
        offset += 4;
    }

    out
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
