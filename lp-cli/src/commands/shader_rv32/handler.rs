//! Handler for `shader-rv32`.

use std::io::Write;

use anyhow::{Context, Result};
use lpir::{FloatMode, validate_module};
use lpvm_native::abi::ModuleAbi;
use lpvm_native::isa::rv32::abi::func_abi_rv32;
use lpvm_native::isa::rv32::debug::disasm::DisasmOptions;
use lpvm_native::isa::rv32fa;
use lpvm_native::isa::rv32fa::debug::physinst;
use lpvm_native::isa::rv32fa::emit::PhysEmitter;
use lpvm_native::{compile_module_asm_text, lower_ops, peephole};

use super::args::ShaderRv32Args;

pub fn handle_shader_rv32(args: ShaderRv32Args) -> Result<()> {
    let src = std::fs::read_to_string(&args.path)
        .with_context(|| format!("read {}", args.path.display()))?;

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

    if args.pipeline == "fast" {
        run_fast_pipeline(&ir, &sig, float_mode, &args)?;
        return Ok(());
    }
    if args.pipeline != "linear" {
        anyhow::bail!("invalid --pipeline (use linear or fast)");
    }

    let asm = compile_module_asm_text(
        &ir,
        &sig,
        float_mode,
        DisasmOptions {
            show_hex_offset: args.hex,
        },
        args.alloc_trace,
    )
    .map_err(|e| anyhow::anyhow!("lpvm-native: {e:?}"))?;

    if let Some(out) = args.output {
        std::fs::write(&out, asm.as_bytes()).with_context(|| format!("write {}", out.display()))?;
    } else {
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(asm.as_bytes())?;
    }

    Ok(())
}

fn run_fast_pipeline(
    ir: &lpir::IrModule,
    sig: &lps_frontend::LpsModuleSig,
    float_mode: FloatMode,
    args: &ShaderRv32Args,
) -> Result<()> {
    let sig_map: std::collections::BTreeMap<&str, &lps_frontend::LpsFnSig> =
        sig.functions.iter().map(|s| (s.name.as_str(), s)).collect();
    let module_abi = ModuleAbi::from_ir_and_sig(ir, sig);

    let mut out = String::new();
    for func in &ir.functions {
        let default_sig = lps_frontend::LpsFnSig {
            name: func.name.clone(),
            return_type: lps_frontend::LpsType::Void,
            parameters: Vec::new(),
        };
        let fn_sig = sig_map
            .get(func.name.as_str())
            .copied()
            .unwrap_or(&default_sig);

        let mut lowered = lower_ops(func, ir, &module_abi, float_mode)
            .map_err(|e| anyhow::anyhow!("lower: {e:?}"))?;
        peephole::optimize(&mut lowered.vinsts);

        if args.show_vinst {
            eprintln!("=== VInst {} ===", func.name);
            for inst in &lowered.vinsts {
                eprintln!("{} {}", inst.mnemonic(), inst.format_alloc_trace_detail());
            }
        }

        let slots = func.total_param_slots() as usize;
        let func_abi = func_abi_rv32(fn_sig, slots);
        let phys = rv32fa::alloc::allocate(&lowered.vinsts, &func_abi, func)
            .map_err(|e| anyhow::anyhow!("fastalloc: {e}"))?;

        if args.show_physinst {
            eprintln!("=== PhysInst {} ===", func.name);
            for inst in &phys {
                eprintln!("{}", physinst::format(inst));
            }
        }

        let code = {
            let mut emitter = PhysEmitter::new();
            for p in &phys {
                emitter.emit(p);
            }
            emitter.finish()
        };

        if args.disassemble {
            eprintln!("=== Disasm {} ===", func.name);
            let mut off = 0usize;
            while off + 4 <= code.len() {
                let w = u32::from_le_bytes(code[off..off + 4].try_into().expect("4 bytes"));
                eprintln!("{:04x}\t{}", off, lp_riscv_inst::format_instruction(w));
                off += 4;
            }
        }

        out.push_str(&format!(".globl\t{}\n", func.name));
        out.push_str(&format!("{}\n", physinst::format_block(&phys)));
        out.push('\n');
    }

    if let Some(out_path) = &args.output {
        std::fs::write(out_path, out.as_bytes())
            .with_context(|| format!("write {}", out_path.display()))?;
    } else {
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(out.as_bytes())?;
    }

    Ok(())
}
