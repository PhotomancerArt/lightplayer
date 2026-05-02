//! [`lpc_source::legacy::LegacyNodeReadRoot`] for [`crate::LpFs`] implementations.

use crate::error::FsError;
use crate::{LpFs, LpFsMemory, LpFsView};
use lpc_model::lp_path::LpPath;
use lpc_source::legacy::LegacyNodeReadRoot;

macro_rules! impl_legacy_node_read_root {
    ($t:ty) => {
        impl LegacyNodeReadRoot for $t {
            type Err = FsError;

            fn read_file(&self, path: &LpPath) -> Result<alloc::vec::Vec<u8>, FsError> {
                LpFs::read_file(self, path)
            }

            fn list_dir(
                &self,
                path: &LpPath,
                recursive: bool,
            ) -> Result<alloc::vec::Vec<lpc_model::LpPathBuf>, FsError> {
                LpFs::list_dir(self, path, recursive)
            }
        }
    };
}

impl_legacy_node_read_root!(LpFsMemory);
impl_legacy_node_read_root!(LpFsView);
impl_legacy_node_read_root!(dyn LpFs);

impl LegacyNodeReadRoot for &(dyn LpFs + '_) {
    type Err = FsError;

    fn read_file(&self, path: &LpPath) -> Result<alloc::vec::Vec<u8>, FsError> {
        LpFs::read_file(*self, path)
    }

    fn list_dir(
        &self,
        path: &LpPath,
        recursive: bool,
    ) -> Result<alloc::vec::Vec<lpc_model::LpPathBuf>, FsError> {
        LpFs::list_dir(*self, path, recursive)
    }
}

#[cfg(feature = "std")]
impl_legacy_node_read_root!(crate::LpFsStd);

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::lp_path::LpPath;
    use lpc_source::legacy::{load_legacy_node_config, nodes::NodeKind};

    #[test]
    fn memory_fs_loads_small_legacy_shader_node_toml() {
        let fs = LpFsMemory::new();
        let node_dir = LpPath::new("/src/demo.shader");
        let config_path = node_dir.to_path_buf().join("node.toml");
        let body = br#"glsl_path = "main.glsl"
texture_spec = "/src/t.texture"
render_order = 0
"#;
        fs.write_file(config_path.as_path(), body).unwrap();

        let (path, cfg) = load_legacy_node_config(&fs, node_dir).unwrap();
        assert_eq!(path.as_str(), "/src/demo.shader");
        assert_eq!(cfg.kind(), NodeKind::Shader);
    }
}
