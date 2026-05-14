use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=src");

    let crate_root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("out dir"));
    let out_file = out_dir.join("slot_shapes.rs");

    lpc_slot_codegen::generate_slot_shapes(lpc_slot_codegen::SlotShapeCodegenConfig {
        crate_root,
        out_file,
    })
    .expect("generate slot shape bootstrap");

    lpc_slot_codegen::generate_mockup_slot_codec(lpc_slot_codegen::MockupSlotCodecCodegenConfig {
        out_file: out_dir.join("generated_slot_codec.rs"),
    })
    .expect("generate mockup slot codec");
}
