//! Test file discovery (walking directories, filtering).

use anyhow::Result;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Discover all test files in the given directory.
/// Finds all files ending in `.glsl`, including `.gen.glsl` files.
pub fn discover_test_files(filetests_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut test_files = Vec::new();

    for entry in WalkDir::new(filetests_dir) {
        let entry = entry?;
        let path = entry.path();

        // Check that it's a file and has .glsl extension
        // This correctly handles both .glsl and .gen.glsl files
        // because path.extension() returns "glsl" for both
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("glsl") {
            test_files.push(path.to_path_buf());
        }
    }

    // Sort for deterministic output
    test_files.sort();

    Ok(test_files)
}

/// Filter test files by path pattern.
pub fn filter_test_files(
    test_files: &[PathBuf],
    filetests_dir: &Path,
    pattern: &str,
) -> Vec<PathBuf> {
    test_files
        .iter()
        .filter(|path| {
            let relative_path = path
                .strip_prefix(filetests_dir)
                .unwrap_or(path)
                .to_string_lossy();
            relative_path.contains(pattern)
        })
        .cloned()
        .collect()
}
