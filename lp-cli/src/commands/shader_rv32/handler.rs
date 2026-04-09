//! Handler for `shader-rv32`.

use std::io::Write;

use anyhow::{Context, Result};
use lpir::{FloatMode, validate_module};
use lpvm_native::compile_module_asm_text;
use lpvm_native::isa::rv32::debug::disasm::DisasmOptions;

use super::args::ShaderRv32Args;

pub fn handle_shader_rv32(args: ShaderRv32Args) -> Result<()> {
    let src = std::fs::read_to_string(&args.path)
        .with_context(|| format!("read {}", args.path.display()))?;

    let naga = lps_frontend::compile(&src).context("GLSL parse (Naga)")?;
    let (ir, _meta) = lps_frontend::lower(&naga).context("lower to LPIR")?;

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

    let asm = compile_module_asm_text(
        &ir,
        float_mode,
        DisasmOptions {
            show_hex_offset: args.hex,
        },
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
