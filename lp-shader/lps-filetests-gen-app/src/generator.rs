//! Main generation dispatch logic.

use crate::cli::Args;
use crate::expand;
use crate::types::{Dimension, VecType};
use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// How to bring the corpus back in sync. Quoted verbatim by `--check` failures.
const REGEN_CMD: &str = "cargo run -p lps-filetests-gen-app -- vec --write";

/// Parsed test file specification.
#[derive(Debug, Clone)]
pub struct TestSpec {
    pub category: String, // e.g., "fn-equal"
    pub vec_type: VecType,
    pub dimension: Dimension,
}

/// Generate test files based on CLI arguments.
pub fn generate(args: &Args) -> Result<()> {
    // A drift gate that only looked at whatever the caller happened to name
    // would not be a gate, so bare `--check` means the whole corpus. Bare
    // dry-run keeps its old "tell me how to use this" behavior.
    let specifiers: Vec<String> = if args.check && args.specifiers.is_empty() {
        vec![String::from("vec")]
    } else {
        args.specifiers.clone()
    };

    if specifiers.is_empty() {
        bail!("No specifiers provided. Use --help for usage information.");
    }

    // Expand specifiers (handles directories, .gen.glsl files, etc.)
    let specs = expand::expand_specifiers(&specifiers)?;

    if specs.is_empty() {
        bail!("No test files to generate for specifiers: {specifiers:?}");
    }

    let filetests_dir = expand::find_filetests_dir()?;
    let outputs = render_outputs(&filetests_dir, &specs)?;

    if args.check {
        return check_outputs(&filetests_dir, &outputs);
    }

    for (output_path, content) in &outputs {
        if args.write {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
            }

            std::fs::write(output_path, content)
                .with_context(|| format!("Failed to write file: {}", output_path.display()))?;

            println!("Generated: {}", output_path.display());
        } else {
            println!("=== {} ===", output_path.display());
            print!("{content}");
            println!();
        }
    }

    // If dry-run, show command to write
    if !args.write {
        println!("\nTo write these files, run:");
        let specifiers_str = specifiers.join(" ");
        println!("  lps-filetests-gen-app {specifiers_str} --write");
    }

    Ok(())
}

/// Render every spec as `output path → contents`.
///
/// Pure in-memory so `--write`, the dry run, `--check` and the determinism test
/// all agree on the exact bytes by construction rather than by convention.
pub fn render_outputs(
    filetests_dir: &Path,
    specs: &[TestSpec],
) -> Result<BTreeMap<PathBuf, String>> {
    let mut outputs = BTreeMap::new();

    for spec in specs {
        let type_name = format_type_name(spec.vec_type, spec.dimension);
        let filename = format!("{}.gen.glsl", spec.category);
        let output_path = filetests_dir.join("vec").join(&type_name).join(&filename);
        outputs.insert(output_path, render_spec(spec)?);
    }

    Ok(outputs)
}

/// Render a single test file's contents.
fn render_spec(spec: &TestSpec) -> Result<String> {
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
        "op-subtract" => crate::vec::op_subtract::generate(spec.vec_type, spec.dimension),
        _ => bail!("Unknown test category: {}", spec.category),
    };

    Ok(content)
}

/// Compare the rendered corpus against what is on disk.
///
/// Three ways to drift, all of them things that have actually happened here:
/// a file edited by hand (`differs`), a file deleted (`missing`), and a file
/// left behind by a generator target that no longer exists (`stale`).
fn check_outputs(filetests_dir: &Path, outputs: &BTreeMap<PathBuf, String>) -> Result<()> {
    let mut drift = Vec::new();

    for (path, expected) in outputs {
        match std::fs::read_to_string(path) {
            Ok(actual) if actual == *expected => {}
            Ok(_) => drift.push(format!("{}: differs", rel(filetests_dir, path))),
            Err(_) => drift.push(format!("{}: missing", rel(filetests_dir, path))),
        }
    }

    for path in existing_gen_files(filetests_dir)? {
        if !outputs.contains_key(&path) {
            drift.push(format!(
                "{}: stale (not produced by this generator)",
                rel(filetests_dir, &path)
            ));
        }
    }

    if drift.is_empty() {
        println!("vec corpus in sync ({} files)", outputs.len());
        return Ok(());
    }

    eprintln!("vec corpus is out of sync with the generator:");
    for line in &drift {
        eprintln!("  {line}");
    }
    eprintln!("\nRegenerate with: {REGEN_CMD}");

    bail!("{} generated vec filetest(s) out of date", drift.len());
}

/// Every `*.gen.glsl` currently on disk under `filetests/vec`, so `--check` can
/// name files the generator no longer produces.
fn existing_gen_files(filetests_dir: &Path) -> Result<Vec<PathBuf>> {
    let root = filetests_dir.join("vec");
    let mut files = Vec::new();

    for entry in WalkDir::new(&root) {
        let entry = entry.with_context(|| format!("Failed to walk: {}", root.display()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(".gen.glsl"))
        {
            files.push(path.to_path_buf());
        }
    }

    files.sort();
    Ok(files)
}

/// Display a corpus path relative to the filetests root, for readable output.
fn rel(filetests_dir: &Path, path: &Path) -> String {
    path.strip_prefix(filetests_dir)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// Format type name for path (e.g., "vec4", "ivec3").
fn format_type_name(vec_type: VecType, dimension: Dimension) -> String {
    crate::vec::util::format_type_name(vec_type, dimension)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The generator has to be a pure function of its specs: `--check` compares
    /// a fresh render against bytes on disk, so any run-to-run instability
    /// (hash iteration order, timestamps) would turn the gate into a flake.
    #[test]
    fn render_outputs_is_deterministic_across_runs() {
        let filetests_dir = expand::find_filetests_dir().expect("filetests dir");
        let specs = expand::expand_specifiers(&[String::from("vec")]).expect("expand vec");

        let first = render_outputs(&filetests_dir, &specs).expect("first render");
        let second = render_outputs(&filetests_dir, &specs).expect("second render");

        assert_eq!(first, second, "generator output is not deterministic");
        assert_eq!(
            first.len(),
            99,
            "the vec corpus is 11 categories x 3 types x 3 dimensions"
        );
    }
}
