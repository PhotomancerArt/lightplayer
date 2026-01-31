//! Utility for expanding test file specifiers into concrete test specifications.
//!
//! Handles:
//! - Directory patterns (e.g., "vec/vec4" -> all tests in that directory)
//! - File paths with .gen.glsl extension
//! - Line number suffixes (e.g., "vec/vec4/fn-equal.gen.glsl:10")
//! - Multiple specifiers

use crate::generator::TestSpec;
use crate::types::{Dimension, VecType};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Expand one or more specifiers into concrete test specifications.
pub fn expand_specifiers(specifiers: &[String]) -> Result<Vec<TestSpec>> {
    let filetests_dir = find_filetests_dir()?;
    let mut all_specs = Vec::new();

    for specifier in specifiers {
        let expanded = expand_single_specifier(specifier.trim(), &filetests_dir)?;
        all_specs.extend(expanded);
    }

    // Deduplicate specs
    all_specs.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    all_specs.dedup_by(|a, b| {
        a.category == b.category && a.vec_type == b.vec_type && a.dimension == b.dimension
    });

    Ok(all_specs)
}

/// Expand a single specifier into test specifications.
fn expand_single_specifier(specifier: &str, filetests_dir: &Path) -> Result<Vec<TestSpec>> {
    // Remove .glsl extension if present
    let specifier = specifier.strip_suffix(".glsl").unwrap_or(specifier);

    // Remove line number suffix if present (e.g., ":10")
    let specifier = specifier.split(':').next().unwrap_or(specifier);

    // Check if it's a .gen.glsl file path
    if specifier.ends_with(".gen") {
        // Extract specifier from .gen.glsl file path
        return extract_spec_from_gen_file(
            specifier.strip_suffix(".gen").unwrap_or(specifier),
            filetests_dir,
        );
    }

    // Try to parse as a direct specifier first (e.g., "vec/vec4/fn-equal", "vec/vec4", "vec")
    // This handles patterns like "vec/vec4" which should generate all tests for vec4
    match parse_specifier(specifier) {
        Ok(specs) if !specs.is_empty() => {
            // Successfully parsed as a specifier pattern
            return Ok(specs);
        }
        Ok(_) => {
            // Parsed but empty - fall through to directory check
        }
        Err(_) => {
            // Failed to parse - fall through to directory check
        }
    }

    // Check if it's a directory pattern (only if parsing failed)
    let full_path = filetests_dir.join(specifier);
    if full_path.is_dir() {
        // Find all .gen.glsl files in this directory
        return find_gen_files_in_dir(&full_path, filetests_dir);
    }

    // If we get here, it's neither a valid specifier nor a directory
    bail!(
        "Invalid specifier: {specifier}. Expected format: vec/vec4/fn-equal, vec/vec4, or a directory path"
    );
}

/// Extract test spec from a .gen.glsl file path.
fn extract_spec_from_gen_file(file_path: &str, filetests_dir: &Path) -> Result<Vec<TestSpec>> {
    // Normalize the path
    let path = if file_path.starts_with('/') || file_path.starts_with("lightplayer/") {
        // Absolute path or path starting with lightplayer/
        PathBuf::from(file_path)
    } else {
        // Relative path - try relative to filetests_dir
        filetests_dir.join(file_path)
    };

    // Make it relative to filetests_dir
    let rel_path = if path.is_absolute() {
        path.strip_prefix(filetests_dir)
            .ok()
            .map(|p| p.to_path_buf())
    } else if path.starts_with(filetests_dir) {
        Some(path.strip_prefix(filetests_dir)?.to_path_buf())
    } else {
        Some(path)
    };

    let rel_path =
        rel_path.ok_or_else(|| anyhow::anyhow!("Could not make path relative: {file_path}"))?;

    // Extract specifier from path like "vec/vec4/fn-equal.gen.glsl"
    // -> "vec/vec4/fn-equal"
    let path_str = rel_path.to_string_lossy();
    let specifier = path_str
        .strip_suffix(".gen.glsl")
        .or_else(|| path_str.strip_suffix(".gen"))
        .unwrap_or(&path_str);

    parse_specifier(specifier)
}

/// Find all .gen.glsl files in a directory and extract their specs.
fn find_gen_files_in_dir(dir: &Path, filetests_dir: &Path) -> Result<Vec<TestSpec>> {
    let mut specs = Vec::new();

    for entry in WalkDir::new(dir) {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "glsl" {
                    // Check if it's a .gen.glsl file
                    if let Some(stem) = path.file_stem() {
                        if stem.to_string_lossy().ends_with(".gen") {
                            // Extract specifier from this file
                            let rel_path = path.strip_prefix(filetests_dir).with_context(|| {
                                format!("Failed to strip prefix from: {}", path.display())
                            })?;

                            let path_str = rel_path.to_string_lossy();
                            let specifier = path_str
                                .strip_suffix(".gen.glsl")
                                .or_else(|| path_str.strip_suffix(".gen"))
                                .unwrap_or(&path_str);

                            match parse_specifier(specifier) {
                                Ok(mut file_specs) => specs.append(&mut file_specs),
                                Err(e) => {
                                    eprintln!(
                                        "Warning: Failed to parse specifier from {}: {}",
                                        path.display(),
                                        e
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(specs)
}

/// Parse a specifier string into test specifications.
fn parse_specifier(specifier: &str) -> Result<Vec<TestSpec>> {
    let parts: Vec<&str> = specifier.split('/').collect();

    match parts.len() {
        1 => {
            // Single part: could be "vec" (folder) or category
            if parts[0] == "vec" {
                // Generate all vec tests
                generate_all_vec_specs()
            } else {
                bail!(
                    "Invalid specifier: {specifier}. Expected format: vec/vec4/fn-equal or vec/vec3"
                );
            }
        }
        2 => {
            // Two parts: "vec/vec3", "vec/ivec3", "vec/uvec3", or "vec/fn-equal"
            let first = parts[0];
            let second = parts[1];

            if first == "vec" {
                // Check if second part is a typed vector (e.g., "vec4", "ivec3", "uvec4")
                // All of these should generate tests for that specific type and dimension
                if let Ok((vec_type, dimension)) = parse_type_and_dimension(second) {
                    // Specific type - generate all categories for this type and dimension
                    generate_all_specs_for_type_and_dimension(vec_type, dimension)
                } else {
                    bail!(
                        "Invalid dimension or type: {second}. Expected vec2, vec3, vec4, ivec2, etc."
                    );
                }
            } else {
                bail!("Invalid specifier: {specifier}. Expected format: vec/vec4/fn-equal");
            }
        }
        3 => {
            // Three parts: "vec/vec4/fn-equal"
            let category = parts[2].to_string();
            let (vec_type, dimension) = parse_type_and_dimension(parts[1])
                .with_context(|| format!("Invalid type/dimension: {}", parts[1]))?;

            Ok(vec![TestSpec {
                category,
                vec_type,
                dimension,
            }])
        }
        _ => {
            bail!("Invalid specifier: {specifier}. Too many path components");
        }
    }
}

/// Parse dimension from string like "vec3", "ivec4", etc.
fn parse_dimension(s: &str) -> Option<Dimension> {
    if s.ends_with('2') {
        Some(Dimension::D2)
    } else if s.ends_with('3') {
        Some(Dimension::D3)
    } else if s.ends_with('4') {
        Some(Dimension::D4)
    } else {
        None
    }
}

/// Parse type and dimension from string like "vec4", "ivec3", "uvec2", "bvec4".
fn parse_type_and_dimension(s: &str) -> Result<(VecType, Dimension)> {
    let vec_type = if s.starts_with("ivec") {
        VecType::IVec
    } else if s.starts_with("uvec") {
        VecType::UVec
    } else if s.starts_with("bvec") {
        VecType::BVec
    } else if s.starts_with("vec") {
        VecType::Vec
    } else {
        bail!("Invalid vector type prefix in: {s}");
    };

    let dimension =
        parse_dimension(s).ok_or_else(|| anyhow::anyhow!("Could not parse dimension from: {s}"))?;

    Ok((vec_type, dimension))
}

/// List of implemented test categories.
fn implemented_categories() -> Vec<&'static str> {
    vec![
        "fn-equal",
        "fn-greater-equal",
        "fn-greater-than",
        "fn-less-equal",
        "fn-less-than",
        "fn-max",
        "fn-min",
        "op-add",
        "op-equal",
        "op-multiply",
    ]
    // TODO: Add more as they're implemented:
}

/// Generate all test specs for all vector types and dimensions.
fn generate_all_vec_specs() -> Result<Vec<TestSpec>> {
    let mut specs = Vec::new();
    let categories = implemented_categories();
    let vec_types = vec![VecType::Vec, VecType::IVec, VecType::UVec];
    let dimensions = vec![Dimension::D2, Dimension::D3, Dimension::D4];

    for category in categories {
        for &vec_type in &vec_types {
            for &dimension in &dimensions {
                specs.push(TestSpec {
                    category: category.to_string(),
                    vec_type,
                    dimension,
                });
            }
        }
    }

    Ok(specs)
}

/// Generate all test specs for a specific type and dimension.
fn generate_all_specs_for_type_and_dimension(
    vec_type: VecType,
    dimension: Dimension,
) -> Result<Vec<TestSpec>> {
    let mut specs = Vec::new();
    let categories = implemented_categories();

    for category in categories {
        specs.push(TestSpec {
            category: category.to_string(),
            vec_type,
            dimension,
        });
    }

    Ok(specs)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Dimension, VecType};

    #[test]
    fn test_parse_dimension() {
        assert_eq!(parse_dimension("vec2"), Some(Dimension::D2));
        assert_eq!(parse_dimension("vec3"), Some(Dimension::D3));
        assert_eq!(parse_dimension("vec4"), Some(Dimension::D4));
        assert_eq!(parse_dimension("ivec2"), Some(Dimension::D2));
        assert_eq!(parse_dimension("uvec4"), Some(Dimension::D4));
        assert_eq!(parse_dimension("invalid"), None);
    }

    #[test]
    fn test_parse_type_and_dimension() {
        assert_eq!(
            parse_type_and_dimension("vec4").unwrap(),
            (VecType::Vec, Dimension::D4)
        );
        assert_eq!(
            parse_type_and_dimension("ivec3").unwrap(),
            (VecType::IVec, Dimension::D3)
        );
        assert_eq!(
            parse_type_and_dimension("uvec2").unwrap(),
            (VecType::UVec, Dimension::D2)
        );
        assert!(parse_type_and_dimension("invalid").is_err());
    }

    #[test]
    fn test_parse_specifier_full() {
        let specs = parse_specifier("vec/vec4/fn-equal").unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].category, "fn-equal");
        assert_eq!(specs[0].vec_type, VecType::Vec);
        assert_eq!(specs[0].dimension, Dimension::D4);
    }

    #[test]
    fn test_parse_specifier_dimension() {
        let specs = parse_specifier("vec/vec3").unwrap();
        // Should generate all categories for vec3
        assert!(specs.len() > 0);
        assert!(specs.iter().all(|s| s.dimension == Dimension::D3));
    }

    #[test]
    fn test_parse_specifier_all_vec() {
        let specs = parse_specifier("vec").unwrap();
        // Should generate all categories, types, and dimensions
        assert!(specs.len() > 0);
    }

    #[test]
    fn test_expand_specifier_vec4() {
        // Test expanding "vec/vec4" - should generate all categories for vec4 only
        let specs = parse_specifier("vec/vec4").unwrap();
        // Should have 10 categories × 1 vector type (vec4) = 10 specs
        assert_eq!(specs.len(), 10);
        assert!(specs.iter().all(|s| s.dimension == Dimension::D4));
        assert!(specs.iter().all(|s| s.vec_type == VecType::Vec));
    }

    #[test]
    fn test_expand_specifier_single_file() {
        // Test expanding a single file specifier
        let specs = parse_specifier("vec/vec4/fn-equal").unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].category, "fn-equal");
        assert_eq!(specs[0].vec_type, VecType::Vec);
        assert_eq!(specs[0].dimension, Dimension::D4);
    }

    #[test]
    fn test_expand_specifier_all_vec() {
        // Test expanding "vec" - should generate all tests
        let specs = parse_specifier("vec").unwrap();
        // Should have 10 categories × 3 vector types × 3 dimensions = 90 specs
        assert_eq!(specs.len(), 90);
    }

    #[test]
    fn test_expand_specifier_ivec3() {
        // Test expanding "vec/ivec3" - should generate all categories for ivec3
        let specs = parse_specifier("vec/ivec3").unwrap();
        assert_eq!(specs.len(), 10); // 10 categories
        assert!(specs.iter().all(|s| s.vec_type == VecType::IVec));
        assert!(specs.iter().all(|s| s.dimension == Dimension::D3));
    }
}
