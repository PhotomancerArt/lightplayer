use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=src");

    let crate_root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("out dir"));

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
