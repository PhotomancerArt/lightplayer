//! Generates the `include_str!` source tables for the palette catalog.
//!
//! Two directories are walked, at `../../assets/palettes/<name>` relative to
//! this crate: `third-party/` and `originals/` (see
//! `assets/palettes/third-party/README.md` for why they're split). Each
//! `*.json` file found becomes one `(id, json_source)` entry in the
//! corresponding generated table.
//!
//! The `third-party/` walk is the one that matters for the isolation
//! contract: a proprietary build may delete that directory wholesale, and
//! this build script must **not** fail when it's gone — it emits an empty
//! table instead of a `include_str!` list that would hard-fail to compile.
//! `originals/` is not expected to ever go missing, but is generated the
//! same way for symmetry (and so a new original just needs a JSON file
//! dropped in the directory, no hand-maintained list to update).

use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let assets_root = manifest_dir.join("../../assets/palettes");

    // Directory-level tracking (catches the directory appearing/disappearing)
    // plus one rerun-if-changed per file below (catches content edits).
    println!("cargo:rerun-if-changed={}", assets_root.display());

    let third_party_dir = assets_root.join("third-party");
    let originals_dir = assets_root.join("originals");

    write_source_table(
        &out_dir.join("third_party_catalog.generated.rs"),
        "THIRD_PARTY_SOURCES",
        &third_party_dir,
    );
    write_source_table(
        &out_dir.join("originals_catalog.generated.rs"),
        "ORIGINAL_SOURCES",
        &originals_dir,
    );
}

/// Walk `dir` recursively for `*.json` files and write a generated Rust file
/// at `out_file` defining `pub const {const_name}: &[(&str, &str)]`, one
/// `(id, include_str!(absolute_path))` entry per file. `id` is the file's
/// path relative to `dir` with the `.json` extension stripped and `/`
/// separators (matches the source's own directory grouping, e.g.
/// `cptcity/jjg-misc/rainfall`).
///
/// If `dir` does not exist, the table is empty — this is the load-bearing
/// behavior that makes deleting `assets/palettes/third-party/` safe.
fn write_source_table(out_file: &Path, const_name: &str, dir: &Path) {
    let mut entries = Vec::new();
    if dir.is_dir() {
        collect_json_files(dir, dir, &mut entries);
    }
    entries.sort();

    let mut code = String::new();
    code.push_str(&format!("pub const {const_name}: &[(&str, &str)] = &[\n"));
    for (id, absolute_path) in &entries {
        // Absolute paths sidestep include_str!'s relative-path resolution
        // ambiguity for files brought in via `include!(concat!(OUT_DIR, ..))`.
        code.push_str(&format!("    ({id:?}, include_str!({absolute_path:?})),\n"));
    }
    code.push_str("];\n");

    fs::write(out_file, code)
        .unwrap_or_else(|error| panic!("write {}: {error}", out_file.display()));
}

fn collect_json_files(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if path.is_dir() {
            collect_json_files(root, &path, out);
            continue;
        }
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        println!("cargo:rerun-if-changed={}", path.display());
        let relative = path
            .strip_prefix(root)
            .expect("walked path must be under root");
        let id = relative
            .with_extension("")
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/");
        let absolute = fs::canonicalize(&path).unwrap_or(path.clone());
        out.push((id, absolute.to_string_lossy().to_string()));
    }
}
