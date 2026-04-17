//! Handler for `shader-debug`.

use anyhow::{Context, Result};
use lp_shader::synth::{SynthError, synthesise_render_texture};
use lpir::inline_weights::{weight_body_len, weight_heavy_bias, weight_markers_zero};
use lpir::{CompilerConfig, FloatMode, LpirModule, validate_module};
use lps_frontend::LpsModuleSig;
use lps_shared::TextureStorageFormat;

use super::args::Args;
use super::collect::{collect_cranelift_data, collect_fa_data};
use super::display::{print_comparison_table, print_detailed_view, print_help_text};
use super::types::{BackendTarget, DebugReport};

pub fn handle_shader_debug(args: Args) -> Result<()> {
    let has_empty_opt = args.opt.iter().any(String::is_empty);
    if has_empty_opt {
        if args.opt.iter().any(|s| !s.is_empty()) {
            anyhow::bail!(
                "`--opt` without KEY=value prints valid keys and values; do not mix with other `--opt` flags on the same command"
            );
        }
        eprintln!("Valid keys for `-o KEY=VALUE` / `--opt KEY=VALUE`:");
        eprintln!();
        eprintln!("  inline.mode                          auto | always | never  (default auto)");
        eprintln!("  inline.always_inline_single_site     true | false           (default true)");
        eprintln!("  inline.small_func_threshold          <usize>                (default 20)");
        eprintln!(
            "  inline.max_growth_budget             <usize>                (default unlimited)"
        );
        eprintln!(
            "  inline.module_op_budget              <usize>                (default unlimited)"
        );
        eprintln!(
            "  q32.add_sub                          saturating | wrapping  (default saturating)"
        );
        eprintln!(
            "  q32.mul                              saturating | wrapping  (default saturating)"
        );
        eprintln!(
            "  q32.div                              saturating | reciprocal (default saturating)"
        );
        return Ok(());
    }

    let src = std::fs::read_to_string(&args.input)
        .with_context(|| format!("read {}", args.input.display()))?;

    let naga = lps_frontend::compile(&src).context("GLSL parse (Naga)")?;
    let (mut ir, mut sig) = lps_frontend::lower(&naga).context("lower to LPIR")?;

    let synth_formats = args
        .render_texture_formats()
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    apply_render_texture_synth(&mut ir, &mut sig, &synth_formats);

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

    let mut compiler_config = CompilerConfig::default();
    for opt in &args.opt {
        let (key, value) = opt.split_once('=').ok_or_else(|| {
            anyhow::anyhow!(
                "--opt expects KEY=VALUE, got: {opt:?} (use `--opt` alone to list valid keys and values)"
            )
        })?;
        compiler_config
            .apply(key, value)
            .map_err(|e| anyhow::anyhow!("invalid --opt: {e}"))?;
    }

    // Parse targets
    let targets = args.targets();
    if targets.is_empty() {
        anyhow::bail!("no valid targets specified. Use: rv32c, rv32n, emu");
    }

    // Get section filter
    let sections = args.sections();
    let func_filter = args.func.as_deref();
    let file_path_str = args.input.to_string_lossy().to_string();

    // Collect data from all targets
    let mut report = DebugReport::new();

    for target in &targets {
        let backend_data = match target {
            BackendTarget::Rv32fa => {
                collect_fa_data(&ir, &sig, float_mode, func_filter, &compiler_config)?
            }
            BackendTarget::Rv32 => {
                collect_cranelift_data(&ir, &sig, float_mode, func_filter, false, &compiler_config)?
            }
            BackendTarget::Emu => {
                collect_cranelift_data(&ir, &sig, float_mode, func_filter, true, &compiler_config)?
            }
        };
        report.backends.push(backend_data);
    }

    if args.weights {
        let by_name: std::collections::BTreeMap<&str, &lpir::IrFunction> = ir
            .functions
            .values()
            .map(|f| (f.name.as_str(), f))
            .collect();
        for backend in &mut report.backends {
            for fd in &mut backend.functions {
                if let Some(func) = by_name.get(fd.name.as_str()) {
                    fd.weight_body_len = weight_body_len(func);
                    fd.weight_mz = weight_markers_zero(func);
                    fd.weight_hb = weight_heavy_bias(func);
                }
            }
        }
    }

    // Print detailed view first (unless summary-only mode)
    if !args.summary {
        print_detailed_view(&report, &sections);

        // Print help text if showing all functions and there's more than one
        if func_filter.is_none() && report.function_names().len() > 1 {
            print_help_text(&file_path_str, &report);
        }
    }

    // Print comparison table at the bottom (always shown)
    print_comparison_table(&report, args.weights);

    Ok(())
}

/// Append `__render_texture_<format>` for each requested format. Best-effort:
/// missing or wrong-arity `render` is a non-fatal info message so the tool
/// stays useful for inputs that aren't full pixel shaders (e.g. LPIR
/// snippets, helper-only files).
fn apply_render_texture_synth(
    ir: &mut LpirModule,
    sig: &mut LpsModuleSig,
    formats: &[TextureStorageFormat],
) {
    if formats.is_empty() {
        return;
    }

    let Some(render_idx) = sig.functions.iter().position(|f| f.name == "render") else {
        eprintln!(
            "info: --render-texture skipped (no `render` function in this input; \
             pass --render-texture none to silence)"
        );
        return;
    };

    for &format in formats {
        match synthesise_render_texture(ir, sig, render_idx, format) {
            Ok(name) => eprintln!("info: synthesised {name}"),
            Err(SynthError::RenderFunctionMissing) => eprintln!(
                "info: --render-texture {format:?} skipped (render arity does not match channel count)",
            ),
            Err(SynthError::InvalidRenderFnIndex) => eprintln!(
                "info: --render-texture {format:?} skipped (internal: render index invalidated)",
            ),
        }
    }
}
