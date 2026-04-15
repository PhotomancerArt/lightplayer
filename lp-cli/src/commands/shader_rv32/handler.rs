//! Handler for `shader-rv32`.

use std::io::Write;

use anyhow::{Context, Result};
use lpir::{FloatMode, validate_module};
use lpvm_native_fa::compile_module_asm_text;
use lpvm_native_fa::rv32::debug::disasm::DisasmOptions;

use super::args::ShaderRv32Args;
use crate::commands::shader_rv32fa::pipeline;

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
    .map_err(|e| anyhow::anyhow!("lpvm-native-fa: {e:?}"))?;

    if let Some(out) = args.output {
        std::fs::write(&out, asm.as_bytes()).with_context(|| format!("write {}", out.display()))?;
    } else {
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(asm.as_bytes())?;
    }

    Ok(())
}

fn run_fast_pipeline(
    ir: &lpir::LpirModule,
    sig: &lps_frontend::LpsModuleSig,
    float_mode: FloatMode,
    args: &ShaderRv32Args,
) -> Result<()> {
    let verbosity =
        pipeline::Verbosity::fast_cli(args.show_vinst, args.show_pinst, args.disassemble);
    let artifact =
        pipeline::run_fastalloc_module(ir, sig, float_mode, verbosity, std::io::stderr().lock())?;

    if let Some(out_path) = &args.output {
        std::fs::write(out_path, artifact.text_assembly.as_bytes())
            .with_context(|| format!("write {}", out_path.display()))?;
    } else {
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(artifact.text_assembly.as_bytes())?;
    }

    Ok(())
}
