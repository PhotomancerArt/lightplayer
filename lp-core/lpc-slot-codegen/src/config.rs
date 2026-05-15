use std::path::PathBuf;

/// Configuration for generating a static slot-shape bootstrap module.
pub struct SlotShapeCodegenConfig {
    pub crate_root: PathBuf,
    pub out_file: PathBuf,
}

/// Configuration for generating typed slot-view helpers.
pub struct SlotViewCodegenConfig {
    pub crate_root: PathBuf,
    pub out_file: PathBuf,
}

/// Configuration for generating `SlotCodec` impls for static slot records.
pub struct SlotCodecCodegenConfig {
    pub crate_root: PathBuf,
    pub out_file: PathBuf,
}
