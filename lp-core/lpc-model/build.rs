use std::path::PathBuf;
use std::{env, fs, io, path::Path};

fn main() {
    let crate_root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("out dir"));
    rerun_if_rust_source_changed(&crate_root.join("src"))
        .expect("track Rust source files for slot codegen");

    lpc_slot_codegen::generate_slot_shapes(lpc_slot_codegen::SlotShapeCodegenConfig {
        crate_root: crate_root.clone(),
        out_file: out_dir.join("slot_shapes.rs"),
    })
    .expect("generate slot shape bootstrap");
    lpc_slot_codegen::generate_slot_views(lpc_slot_codegen::SlotViewCodegenConfig {
        crate_root,
        out_file: out_dir.join("slot_views.rs"),
    })
    .expect("generate slot views");
}

fn rerun_if_rust_source_changed(path: &Path) -> io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            rerun_if_rust_source_changed(&path)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    Ok(())
}
