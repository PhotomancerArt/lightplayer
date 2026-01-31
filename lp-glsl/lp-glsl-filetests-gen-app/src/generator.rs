//! Main generation dispatch logic.

use crate::cli::Args;
use crate::expand;
use crate::types::{Dimension, VecType};
use anyhow::{Context, Result, bail};
use std::path::PathBuf;

/// Parsed test file specification.
#[derive(Debug, Clone)]
pub struct TestSpec {
    pub category: String, // e.g., "fn-equal"
    pub vec_type: VecType,
    pub dimension: Dimension,
}

/// Generate test files based on CLI arguments.
pub fn generate(args: &Args) -> Result<()> {
    if args.specifiers.is_empty() {
        bail!("No specifiers provided. Use --help for usage information.");
    }

    // Expand specifiers (handles directories, .gen.glsl files, etc.)
    let specs = expand::expand_specifiers(&args.specifiers)?;

    if specs.is_empty() {
        bail!(
            "No test files to generate for specifiers: {:?}",
            args.specifiers
        );
    }

    // Generate each test file
    for spec in &specs {
        generate_test_file(spec, args.write)?;
    }

    // If dry-run, show command to write
    if !args.write {
        println!("\nTo write these files, run:");
        let specifiers_str = args.specifiers.join(" ");
        println!("  lp-glsl-filetests-gen-app {specifiers_str} --write");
    }

    Ok(())
}

/// Generate a single test file.
fn generate_test_file(spec: &TestSpec, write: bool) -> Result<()> {
    // Determine output path
    let filetests_dir = find_filetests_dir()?;
    let type_name = format_type_name(spec.vec_type, spec.dimension);
    let filename = format!("{}.gen.glsl", spec.category);
    let output_path = filetests_dir.join("vec").join(&type_name).join(&filename);

    // Generate content
    let content = match spec.category.as_str() {
        "fn-equal" => crate::vec::fn_equal::generate(spec.vec_type, spec.dimension),
        "fn-greater-equal" => crate::vec::fn_greater_equal::generate(spec.vec_type, spec.dimension),
        "fn-greater-than" => crate::vec::fn_greater_than::generate(spec.vec_type, spec.dimension),
        "fn-less-equal" => crate::vec::fn_less_equal::generate(spec.vec_type, spec.dimension),
        "fn-less-than" => crate::vec::fn_less_than::generate(spec.vec_type, spec.dimension),
        "fn-max" => crate::vec::fn_max::generate(spec.vec_type, spec.dimension),
        "fn-min" => crate::vec::fn_min::generate(spec.vec_type, spec.dimension),
        "op-add" => crate::vec::op_add::generate(spec.vec_type, spec.dimension),
        "op-equal" => crate::vec::op_equal::generate(spec.vec_type, spec.dimension),
        "op-multiply" => crate::vec::op_multiply::generate(spec.vec_type, spec.dimension),
        _ => {
            return Err(anyhow::anyhow!("Unknown test category: {}", spec.category));
        }
    };

    if write {
        // Create directory if needed
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        // Write file
        std::fs::write(&output_path, content)
            .with_context(|| format!("Failed to write file: {}", output_path.display()))?;

        println!("Generated: {}", output_path.display());
    } else {
        // Dry-run: print content
        println!("=== {} ===", output_path.display());
        print!("{content}");
        println!();
    }

    Ok(())
}

/// Format type name for path (e.g., "vec4", "ivec3").
fn format_type_name(vec_type: VecType, dimension: Dimension) -> String {
    crate::vec::util::format_type_name(vec_type, dimension)
}

/// Find the filetests directory.
fn find_filetests_dir() -> Result<PathBuf> {
    // Look for filetests directory relative to current working directory
    // Try common locations
    let candidates = vec![
        PathBuf::from("lightplayer/crates/lp-glsl-filetests/filetests"),
        PathBuf::from("crates/lp-glsl-filetests/filetests"),
        PathBuf::from("../lp-glsl-filetests/filetests"),
    ];

    for candidate in candidates {
        if candidate.exists() && candidate.is_dir() {
            return Ok(candidate);
        }
    }

    // Try to find it from current directory
    let current_dir = std::env::current_dir()?;
    let mut search_dir = current_dir.as_path();

    loop {
        let candidate = search_dir.join("lightplayer/crates/lp-glsl-filetests/filetests");
        if candidate.exists() && candidate.is_dir() {
            return Ok(candidate);
        }

        if let Some(parent) = search_dir.parent() {
            search_dir = parent;
        } else {
            break;
        }
    }

    bail!("Could not find filetests directory. Please run from workspace root.");
}
