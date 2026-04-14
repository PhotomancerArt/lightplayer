//! Allocator filetests for fastalloc.
//!
//! Discovers `.lpir` files in `filetests/alloc/`. Filesystem I/O stays here so the library
//! stays `no_std`; only this integration test binary needs `std`.
//!
//! Run: `cargo test -p lpvm-native-fa --test filetests`
//!
//! BLESS: `BLESS=1 cargo test -p lpvm-native-fa --test filetests`

use lpvm_native_fa::filetest::{
    FILETEST_SEPARATOR, compute_filetest_snapshot, parse_filetest, run_filetest,
};
use std::env;
use std::path::{Path, PathBuf};

fn discover_filetests(dir: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                result.extend(discover_filetests(&path));
            } else if path.extension().is_some_and(|ext| ext == "lpir") {
                result.push(path);
            }
        }
    }

    result.sort();
    result
}

fn bless_filetest(path: &Path, new_snapshot: &str) -> Result<(), String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

    let parts: Vec<&str> = content.splitn(2, FILETEST_SEPARATOR).collect();
    if parts.len() != 2 {
        return Err(format!("Could not find separator in {}", path.display()));
    }

    let header = parts[0];
    let header = if header.ends_with('\n') {
        header.to_string()
    } else {
        format!("{header}\n")
    };

    let new_content = format!("{header}{new_snapshot}\n");
    std::fs::write(path, new_content)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
    println!("Blessed: {}", path.display());
    Ok(())
}

#[test]
fn filetest_alloc_snapshot() {
    let bless = env::var("BLESS").is_ok();
    let mut failures: Vec<String> = Vec::new();

    let test_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("filetests")
        .join("alloc");

    let tests = discover_filetests(&test_dir);

    if tests.is_empty() {
        println!("No .lpir filetests found in {}", test_dir.display());
        return;
    }

    for path in &tests {
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("Failed to read {:?}: {}", path, e));

        let test = parse_filetest(&path.to_string_lossy(), &content);

        let result = if bless {
            match compute_filetest_snapshot(&test) {
                Ok(actual) => bless_filetest(path, &actual),
                Err(e) => Err(e),
            }
        } else {
            run_filetest(&test)
        };

        if let Err(msg) = result {
            failures.push(format!("{}:\n{}", test.name, msg));
        }
    }

    if !failures.is_empty() {
        panic!(
            "{} filetest(s) failed:\n\n{}",
            failures.len(),
            failures.join("\n\n---\n\n")
        );
    }

    println!("{} filetests passed", tests.len());
}

#[test]
fn list_filetests() {
    let test_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("filetests")
        .join("alloc");

    let tests = discover_filetests(&test_dir);

    println!("Discovered {} filetests in {:?}:", tests.len(), test_dir);
    for path in &tests {
        println!("  - {:?}", path.file_stem().unwrap_or_default());
    }

    if tests.is_empty() {
        println!("  (none found - create .lpir files in filetests/alloc/)");
    }
}
