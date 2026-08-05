//! Enforces the isolation contract from `assets/palettes/third-party/
//! README.md`:
//!
//! 1. Every third-party palette has a matching license row in
//!    `assets/palettes/third-party/COPYING.md`.
//! 2. No source file in the repository — other than this crate's own
//!    `third_party.rs` / `build.rs` — references the third-party asset
//!    path.

use std::fs;
use std::path::{Path, PathBuf};

use lpa_palettes::{PaletteCategory, all_palettes};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

#[test]
fn every_third_party_palette_has_a_license_entry() {
    let copying = fs::read_to_string(repo_root().join("assets/palettes/third-party/COPYING.md"))
        .expect("read assets/palettes/third-party/COPYING.md");

    for palette in all_palettes() {
        let is_third_party = matches!(
            palette.category,
            PaletteCategory::FastledStock | PaletteCategory::CptCity
        );
        if !is_third_party {
            continue;
        }
        let license = palette
            .license
            .as_ref()
            .unwrap_or_else(|| panic!("third-party palette {:?} has no license", palette.id));

        // The manifest table rows are `| `id` | name | spdx | author |
        // <source_url> | stops |` — a loose but real cross-check: the id,
        // the spdx tag, and the source URL must each appear together on
        // one line.
        let row = copying
            .lines()
            .find(|line| line.contains(&format!("`{}`", palette.id)))
            .unwrap_or_else(|| {
                panic!(
                    "COPYING.md has no row for palette {:?} ({})",
                    palette.id, palette.name
                )
            });

        assert!(
            row.contains(&license.spdx),
            "COPYING.md row for {:?} does not mention license {:?}:\n{row}",
            palette.id,
            license.spdx
        );
        assert!(
            row.contains(license.source_url.as_str()),
            "COPYING.md row for {:?} does not mention source URL {:?}:\n{row}",
            palette.id,
            license.source_url
        );
    }
}

#[test]
fn every_copying_md_row_matches_a_real_catalog_entry() {
    // The reverse direction: catch a COPYING.md row left behind after a
    // palette was removed (the manifest silently claiming a license for
    // something no longer shipped).
    let copying = fs::read_to_string(repo_root().join("assets/palettes/third-party/COPYING.md"))
        .expect("read COPYING.md");
    let known_ids: Vec<&str> = all_palettes().iter().map(|p| p.id.as_str()).collect();

    for line in copying.lines() {
        if !line.starts_with("| `") {
            continue;
        }
        let id = line
            .trim_start_matches("| `")
            .split('`')
            .next()
            .expect("row starts with | `id`");
        assert!(
            known_ids.contains(&id),
            "COPYING.md lists {id:?} but no catalog palette has that id"
        );
    }
}

#[test]
fn no_source_file_outside_the_loader_references_third_party_palettes() {
    // Grep-style boundary test, matching the repo's established idiom
    // (`scripts/check-serde-content.sh`'s allowlist-and-grep shape) rather
    // than a new dependency-graph tool.
    //
    // Scope: Rust *code* lines only (doc comments / prose mentioning the
    // isolation directory by name — e.g. explaining the rule in a doc
    // comment, or this repo's own README/COPYING.md naming themselves —
    // are expected and fine; the rule is about code that reads the data,
    // not text that names the path). A line only counts as a reference if
    // it isn't a `//`-comment line and mentions the path.
    let root = repo_root();
    let needle = "assets/palettes/third-party";

    let allowlisted_suffixes = [
        "lp-app/lpa-palettes/build.rs",
        "lp-app/lpa-palettes/src/third_party.rs",
        "lp-app/lpa-palettes/tests/license_manifest.rs",
    ];

    let mut offending = Vec::new();
    walk_rust_files(&root, &mut |path, contents| {
        let relative = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if allowlisted_suffixes
            .iter()
            .any(|suffix| relative.ends_with(suffix))
        {
            return;
        }
        for (line_number, line) in contents.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue; // doc comment / line comment: prose, not code.
            }
            if line.contains(needle) {
                offending.push(format!("{relative}:{}", line_number + 1));
            }
        }
    });

    assert!(
        offending.is_empty(),
        "only the catalog loader may reference {needle} in code; found it in: {offending:?}"
    );
}

/// Walk `.rs` files, skipping build output, VCS metadata, and the repo's
/// own vendored `third_party/` (unrelated: C/Rust upstream sources, not
/// this crate's palette isolation directory).
fn walk_rust_files(dir: &Path, visit: &mut impl FnMut(&Path, &str)) {
    const SKIP_DIRS: &[&str] = &[
        "target",
        ".git",
        "node_modules",
        "dist",
        ".jj",
        "third_party", // vendored C/Rust sources under the repo's own third_party/
    ];

    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_ref()) {
                continue;
            }
            walk_rust_files(&path, visit);
            continue;
        }
        let is_rust = path.extension().and_then(|ext| ext.to_str()) == Some("rs");
        if !is_rust {
            continue;
        }
        if let Ok(contents) = fs::read_to_string(&path) {
            visit(&path, &contents);
        }
    }
}
