//! Fastalloc RV32FA pipeline shared by `shader-rv32 --pipeline fast` and `shader-rv32fa`.

use std::io::Write;

use anyhow::Result;
use lpir::FloatMode;
use lpvm_native_fa::abi::ModuleAbi;
use lpvm_native_fa::emit::emit_lowered_with_alloc;
use lpvm_native_fa::fa_alloc;
use lpvm_native_fa::fa_alloc::liveness::{analyze_liveness, format_liveness};
use lpvm_native_fa::fa_alloc::render::render_alloc_output;
use lpvm_native_fa::rv32::abi::func_abi_rv32;
use lpvm_native_fa::rv32::debug::region::format_region_tree;
use lpvm_native_fa::{lower_ops, peephole};

/// Which stderr debug sections to print.
#[derive(Clone, Copy, Debug)]
pub struct Verbosity {
    pub vinst: bool,
    pub pinst: bool,
    pub disasm: bool,
    pub region: bool,
    pub liveness: bool,
}

impl Verbosity {
    /// Match legacy `shader-rv32 --pipeline fast` opt-in flags.
    pub fn fast_cli(show_vinst: bool, show_pinst: bool, disassemble: bool) -> Self {
        Self {
            vinst: show_vinst,
            pinst: show_pinst,
            disasm: disassemble,
            region: false,
            liveness: false,
        }
    }
}

pub struct FastAllocArtifact {
    pub text_assembly: String,
    pub machine_code: Vec<u8>,
}

pub fn run_fastalloc_module(
    ir: &lpir::LpirModule,
    sig: &lps_frontend::LpsModuleSig,
    float_mode: FloatMode,
    verbosity: Verbosity,
    mut debug: impl Write,
) -> Result<FastAllocArtifact> {
    let sig_map: std::collections::BTreeMap<&str, &lps_frontend::LpsFnSig> =
        sig.functions.iter().map(|s| (s.name.as_str(), s)).collect();
    let module_abi = ModuleAbi::from_ir_and_sig(ir, sig);

    let mut text_assembly = String::new();
    let mut machine_code = Vec::new();

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

        if verbosity.vinst {
            writeln!(debug, "=== VInst {} ===", func.name)?;
            for inst in &lowered.vinsts {
                writeln!(
                    debug,
                    "{} {}",
                    inst.mnemonic(),
                    inst.format_alloc_trace_detail(&lowered.vreg_pool, &lowered.symbols)
                )?;
            }
        }

        if verbosity.region {
            writeln!(debug, "=== Region Tree {} ===", func.name)?;
            let text = format_region_tree(
                &lowered.region_tree,
                lowered.region_tree.root,
                &lowered.vinsts,
                &lowered.vreg_pool,
                &lowered.symbols,
                0,
            );
            writeln!(debug, "{}", text)?;
        }

        if verbosity.liveness {
            writeln!(debug, "=== Liveness {} ===", func.name)?;
            let liveness = analyze_liveness(
                &lowered.region_tree,
                lowered.region_tree.root,
                &lowered.vinsts,
                &lowered.vreg_pool,
            );
            writeln!(debug, "{}", format_liveness(&liveness))?;
        }

        let slots = func.total_param_slots() as usize;
        let func_abi = func_abi_rv32(fn_sig, slots);
        let alloc_result = fa_alloc::allocate(&lowered, &func_abi)
            .map_err(|e| anyhow::anyhow!("fastalloc: {e}"))?;

        // Print alloc trace if requested via env var
        if std::env::var("LPVM_ALLOC_TRACE").unwrap_or_default() == "1"
            && !alloc_result.output.trace.is_empty()
        {
            writeln!(debug, "{}", alloc_result.output.trace.format())?;
        }

        if verbosity.pinst {
            writeln!(debug, "=== Alloc snapshot {} ===", func.name)?;
            let text = render_alloc_output(
                &lowered.vinsts,
                &lowered.vreg_pool,
                &alloc_result.output,
                Some(&lowered.symbols),
            );
            writeln!(debug, "{}", text)?;
        }

        let emitted = emit_lowered_with_alloc(&lowered, &func_abi, alloc_result)
            .map_err(|e| anyhow::anyhow!("emit: {e:?}"))?;
        let code = emitted.code;

        if verbosity.disasm {
            writeln!(debug, "=== Disasm {} ===", func.name)?;
            let mut off = 0usize;
            while off + 4 <= code.len() {
                let w = u32::from_le_bytes(code[off..off + 4].try_into().expect("4 bytes"));
                writeln!(
                    debug,
                    "{:04x}\t{}",
                    off,
                    lp_riscv_inst::format_instruction(w)
                )?;
                off += 4;
            }
        }

        text_assembly.push_str(&format!(".globl\t{}\n", func.name));
        let mut off = 0usize;
        while off + 4 <= code.len() {
            let w = u32::from_le_bytes(code[off..off + 4].try_into().expect("4 bytes"));
            text_assembly.push_str(&format!(
                "\t{:08x}\t{}\n",
                w,
                lp_riscv_inst::format_instruction(w)
            ));
            off += 4;
        }
        text_assembly.push('\n');

        machine_code.extend_from_slice(&code);
    }

    Ok(FastAllocArtifact {
        text_assembly,
        machine_code,
    })
}
