//! GLSL file → LPIR text (parse, lower, validate, print).

use std::io::Write;

use anyhow::{Context, Result};

use super::args::ShaderLpirArgs;
use lpir::{print_module, validate_module};

pub fn handle_shader_lpir(args: ShaderLpirArgs) -> Result<()> {
    let src = std::fs::read_to_string(&args.path)
        .with_context(|| format!("read {}", args.path.display()))?;

    let naga = lps_frontend::compile(&src).context("GLSL parse (Naga)")?;
    let (ir, _meta) = lps_frontend::lower(&naga).context("lower to LPIR")?;

    if let Err(errs) = validate_module(&ir) {
        if args.skip_validate {
            let mut stderr = std::io::stderr().lock();
            writeln!(
                stderr,
                "# warning: LPIR validation failed (--skip-validate); printing anyway:"
            )?;
            for e in &errs {
                writeln!(stderr, "#   {e}")?;
            }
        } else {
            anyhow::bail!(
                "LPIR validation failed:\n{}",
                errs.iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }

    print!("{}", print_module(&ir));

    if args.stats {
        let mut stderr = std::io::stderr().lock();
        writeln!(
            stderr,
            "# LPIR stats for {} ({} functions)",
            args.path.display(),
            ir.functions.len()
        )?;
        for f in ir.functions.values() {
            let vregs = f.vreg_types.len();
            let ops = f.body.len();
            writeln!(
                stderr,
                "# @{name}: ops={ops} vregs={vregs} slots={slots}",
                name = f.name,
                slots = f.slots.len(),
            )?;
        }
        let total_ops: usize = ir.functions.values().map(|f| f.body.len()).sum();
        let max_vregs = ir
            .functions
            .values()
            .map(|f| f.vreg_types.len())
            .max()
            .unwrap_or(0);
        writeln!(
            stderr,
            "# total ops={total_ops} max vregs (single function)={max_vregs}"
        )?;
    }

    Ok(())
}
