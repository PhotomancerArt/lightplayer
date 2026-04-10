//! Handler for `shader-rv32fa`.

use std::io::Write;

use anyhow::{Context, Result};
use lpir::{FloatMode, print_module, validate_module};

use super::args::{Args, ArtifactFormat};
use super::pipeline;

pub fn handle_shader_rv32fa(args: Args) -> Result<()> {
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

    if args.show_lpir() {
        eprintln!("=== LPIR ===\n{}", print_module(&ir));
    }

    let v = args.verbosity();
    let artifact =
        pipeline::run_fastalloc_module(&ir, &sig, float_mode, v, std::io::stderr().lock())?;

    write_artifact(&args, &artifact)
}

fn write_artifact(args: &Args, artifact: &pipeline::FastAllocArtifact) -> Result<()> {
    let data: std::borrow::Cow<'_, [u8]> = match args.format {
        ArtifactFormat::Text => std::borrow::Cow::Borrowed(artifact.text_assembly.as_bytes()),
        ArtifactFormat::Bin => std::borrow::Cow::Borrowed(&artifact.machine_code),
        ArtifactFormat::Hex => std::borrow::Cow::Owned(hex_words_lines(&artifact.machine_code)),
    };

    if let Some(path) = &args.output {
        std::fs::write(path, data.as_ref()).with_context(|| format!("write {}", path.display()))?;
    } else {
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(data.as_ref())?;
    }

    Ok(())
}

fn hex_words_lines(code: &[u8]) -> Vec<u8> {
    use std::fmt::Write;

    let mut s = String::new();
    let mut chunks = code.chunks_exact(4);
    for chunk in chunks.by_ref() {
        let w = u32::from_le_bytes(chunk.try_into().expect("4 bytes"));
        let _ = writeln!(&mut s, "{w:08x}");
    }
    let rem = chunks.remainder();
    if !rem.is_empty() {
        let _ = write!(&mut s, "incomplete tail:");
        for b in rem {
            let _ = write!(&mut s, " {b:02x}");
        }
        let _ = writeln!(&mut s);
    }
    s.into_bytes()
}
