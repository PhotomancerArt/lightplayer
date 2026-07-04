//! Throwaway TOML→JSON node-artifact converter (remove-toml migration, P3).
//!
//! Walks the given roots, converts every `*.toml` node artifact to canonical
//! pretty JSON via the model (read_toml → rewrite refs → write_json), writes
//! `<stem>.json`, and deletes the `.toml`. Deleted in P8.

use std::path::{Path, PathBuf};

use lpc_model::{ArtifactPath, NodeDef, NodeInvocation, NodeInvocationSlot, SlotShapeRegistry};

fn main() {
    let roots: Vec<String> = std::env::args().skip(1).collect();
    if roots.is_empty() {
        eprintln!("usage: convert-toml-artifacts <root-dir>...");
        std::process::exit(2);
    }
    let registry = SlotShapeRegistry::default();
    let mut converted = 0usize;
    let mut failed = 0usize;
    for root in roots {
        for path in collect_toml_files(Path::new(&root)) {
            match convert_file(&registry, &path) {
                Ok(()) => converted += 1,
                Err(message) => {
                    failed += 1;
                    eprintln!("FAIL {}: {message}", path.display());
                }
            }
        }
    }
    println!("converted {converted} artifact(s), {failed} failure(s)");
    if failed > 0 {
        std::process::exit(1);
    }
}

fn collect_toml_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) => {
                eprintln!("skip {}: {error}", dir.display());
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "toml")
                && path.file_name().is_some_and(|name| name != "Cargo.toml")
            {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn convert_file(registry: &SlotShapeRegistry, path: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut def = NodeDef::read_toml(registry, &text).map_err(|error| error.to_string())?;
    rewrite_refs(&mut def);
    let json = def.write_json(registry).map_err(|error| error.to_string())?;
    let target = path.with_extension("json");
    std::fs::write(&target, json).map_err(|error| error.to_string())?;
    std::fs::remove_file(path).map_err(|error| error.to_string())?;
    println!("{} -> {}", path.display(), target.display());
    Ok(())
}

fn rewrite_refs(def: &mut NodeDef) {
    match def {
        NodeDef::Project(project) => {
            for (_, invocation) in project.nodes.entries.iter_mut() {
                rewrite_invocation(invocation);
            }
        }
        NodeDef::Playlist(playlist) => {
            for (_, entry) in playlist.entries.entries.iter_mut() {
                rewrite_invocation(&mut entry.node);
            }
        }
        _ => {}
    }
}

fn rewrite_invocation(slot: &mut NodeInvocationSlot) {
    if let NodeInvocation::Ref(path) = slot.value_mut() {
        let current = path.value().as_str();
        if let Some(stem) = current.strip_suffix(".toml") {
            let rewritten = format!("{stem}.json");
            path.set(ArtifactPath(rewritten));
        }
    }
}
